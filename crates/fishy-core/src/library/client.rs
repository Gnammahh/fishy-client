use super::{build_library, worth_fetching, CatalogItem, Game, LibraryItem};
use crate::auth::client::{http_client, network, read_json};
use crate::auth::{AuthError, Secret};
use futures::stream::{self, StreamExt};
use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;

pub const LIBRARY_HOST: &str = "https://library-service.live.use1a.on.epicgames.com";
pub const CATALOG_HOST: &str = "https://catalog-public-service-prod06.ol.epicgames.com";
/// How many catalog lookups run at the same time.
const PARALLEL_LOOKUPS: usize = 8;

/// Reads the user's library and game info from Epic.
#[derive(Clone)]
pub struct LibraryClient {
    http: Client,
    library_base: String,
    catalog_base: String,
}

impl Default for LibraryClient {
    fn default() -> Self {
        Self::new()
    }
}

impl LibraryClient {
    pub fn new() -> Self {
        Self::with_base_urls(LIBRARY_HOST, CATALOG_HOST)
    }

    /// For tests: point both services at a fake Epic server.
    pub fn with_base_urls(library: &str, catalog: &str) -> Self {
        Self {
            http: http_client(),
            library_base: library.trim_end_matches('/').to_string(),
            catalog_base: catalog.trim_end_matches('/').to_string(),
        }
    }

    /// Every library record, following `nextCursor` page by page.
    pub async fn items(&self, token: &Secret) -> Result<Vec<LibraryItem>, AuthError> {
        let mut all = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut req = self
                .http
                .get(format!("{}/library/api/public/items", self.library_base))
                .header("Authorization", format!("bearer {}", token.expose()))
                .query(&[("includeMetadata", "true")]);
            if let Some(c) = &cursor {
                req = req.query(&[("cursor", c.as_str())]);
            }
            let json = read_json(req.send().await.map_err(network)?).await?;
            let records: Vec<LibraryItem> = serde_json::from_value(json.get("records").cloned().unwrap_or(Value::Array(vec![])))
                .map_err(|e| AuthError::Unexpected(e.to_string()))?;
            all.extend(records);
            cursor = json
                .pointer("/responseMetadata/nextCursor")
                .and_then(Value::as_str)
                .filter(|c| !c.is_empty())
                .map(str::to_string);
            if cursor.is_none() {
                return Ok(all);
            }
        }
    }

    /// Game info for one item, or None if Epic has none.
    pub async fn catalog_item(&self, token: &Secret, namespace: &str, id: &str) -> Result<Option<CatalogItem>, AuthError> {
        let res = self
            .http
            .get(format!("{}/catalog/api/shared/namespace/{namespace}/bulk/items", self.catalog_base))
            .header("Authorization", format!("bearer {}", token.expose()))
            .query(&[
                ("id", id),
                ("includeDLCDetails", "true"),
                ("includeMainGameDetails", "true"),
                ("country", "US"),
                ("locale", "en"),
            ])
            .send()
            .await
            .map_err(network)?;
        let json = read_json(res).await?;
        Ok(json.get(id).cloned().and_then(|v| serde_json::from_value(v).ok()))
    }

    /// The whole Library: items, then game info (8 at a time). A failed lookup keeps the game.
    pub async fn fetch_library(&self, token: &Secret) -> Result<Vec<Game>, AuthError> {
        let items = self.items(token).await?;
        let wanted: Vec<&LibraryItem> = items.iter().filter(|i| worth_fetching(i)).collect();
        let catalog: HashMap<String, CatalogItem> = stream::iter(wanted)
            .map(|i| async move {
                let info = self.catalog_item(token, &i.namespace, &i.catalog_item_id).await.ok().flatten();
                (i.catalog_item_id.clone(), info)
            })
            .buffer_unordered(PARALLEL_LOOKUPS)
            .filter_map(|(id, info)| async move { info.map(|i| (id, i)) })
            .collect()
            .await;
        Ok(build_library(&items, &catalog))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::Secret;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn items_follow_the_cursor_and_send_the_bearer_token() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/library/api/public/items"))
            .and(query_param("includeMetadata", "true"))
            .and(query_param("cursor", "page2"))
            .and(header("authorization", "bearer eg1~A"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "records": [{"namespace": "n2", "catalogItemId": "b", "appName": "B"}],
                "responseMetadata": {}
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/library/api/public/items"))
            .and(header("authorization", "bearer eg1~A"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "records": [{"namespace": "n1", "catalogItemId": "a", "appName": "A", "sandboxType": "PUBLIC"}],
                "responseMetadata": {"nextCursor": "page2"}
            })))
            .expect(1)
            .mount(&server)
            .await;
        let c = LibraryClient::with_base_urls(&server.uri(), &server.uri());
        let items = c.items(&Secret::new("eg1~A")).await.unwrap();
        let ids: Vec<&str> = items.iter().map(|i| i.catalog_item_id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b"]);
    }

    #[tokio::test]
    async fn fetch_library_combines_items_and_catalog_and_survives_a_failed_lookup() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/library/api/public/items"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "records": [
                    {"namespace": "n1", "catalogItemId": "a", "appName": "AppA"},
                    {"namespace": "n2", "catalogItemId": "b", "appName": "AppB"},
                    {"namespace": "ue", "catalogItemId": "u", "appName": "Asset"}
                ],
                "responseMetadata": {}
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/catalog/api/shared/namespace/n1/bulk/items"))
            .and(query_param("id", "a"))
            .and(query_param("includeMainGameDetails", "true"))
            .and(header("authorization", "bearer eg1~A"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "a": {"title": "Tidepool Odyssey", "keyImages": [{"type": "DieselGameBoxTall", "url": "https://cdn/a.jpg"}], "categories": [{"path": "games"}]}
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/catalog/api/shared/namespace/n2/bulk/items"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/catalog/api/shared/namespace/ue/bulk/items"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0) // Unreal assets are never looked up
            .mount(&server)
            .await;
        let c = LibraryClient::with_base_urls(&server.uri(), &server.uri());
        let games = c.fetch_library(&Secret::new("eg1~A")).await.unwrap();
        assert_eq!(games.len(), 2);
        assert_eq!(games[0].title, "AppB"); // fallback title, sorted A-Z
        assert_eq!(games[1].title, "Tidepool Odyssey");
        assert_eq!(games[1].cover_url.as_deref(), Some("https://cdn/a.jpg"));
    }

    #[tokio::test]
    async fn library_error_is_rejected() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({"errorCode": "errors.com.epicgames.common.authentication.token_verification_failed"})))
            .mount(&server)
            .await;
        let c = LibraryClient::with_base_urls(&server.uri(), &server.uri());
        assert!(matches!(c.fetch_library(&Secret::new("x")).await, Err(AuthError::Rejected { .. })));
    }
}
