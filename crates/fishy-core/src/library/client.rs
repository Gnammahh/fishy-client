use super::{build_library, worth_fetching, CatalogItem, Game, LibraryItem};
use crate::auth::client::{http_client, network, read_json};
use crate::auth::{AuthError, Secret};
use futures::stream::{self, StreamExt};
use reqwest::Client;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub const LIBRARY_HOST: &str = "https://library-service.live.use1a.on.epicgames.com";
pub const CATALOG_HOST: &str = "https://catalog-public-service-prod06.ol.epicgames.com";
pub const LAUNCHER_HOST: &str = crate::install::client::LAUNCHER_HOST;
/// How many catalog lookups run at the same time.
const PARALLEL_LOOKUPS: usize = 8;

/// Reads the user's library and game info from Epic.
#[derive(Clone)]
pub struct LibraryClient {
    http: Client,
    library_base: String,
    catalog_base: String,
    launcher_base: String,
}

impl Default for LibraryClient {
    fn default() -> Self {
        Self::new()
    }
}

impl LibraryClient {
    pub fn new() -> Self {
        Self::with_base_urls(LIBRARY_HOST, CATALOG_HOST, LAUNCHER_HOST)
    }

    /// For tests: point the services at a fake Epic server.
    pub fn with_base_urls(library: &str, catalog: &str, launcher: &str) -> Self {
        Self {
            http: http_client(),
            library_base: library.trim_end_matches('/').to_string(),
            catalog_base: catalog.trim_end_matches('/').to_string(),
            launcher_base: launcher.trim_end_matches('/').to_string(),
        }
    }

    /// App names that have a Windows build (Epic's asset list, like Legendary's `get_game_assets`).
    pub async fn windows_apps(&self, token: &Secret) -> Result<HashSet<String>, AuthError> {
        let res = self
            .http
            .get(format!("{}/launcher/api/public/assets/Windows", self.launcher_base))
            .header("Authorization", format!("bearer {}", token.expose()))
            .query(&[("label", "Live")])
            .send()
            .await
            .map_err(network)?;
        let json = read_json(res).await?;
        let assets = json.as_array().ok_or_else(|| AuthError::Unexpected("asset list is not a list".into()))?;
        Ok(assets.iter().filter_map(|a| a.get("appName")?.as_str()).map(str::to_string).collect())
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

    /// The whole Library: items, then game info (8 at a time). A failed lookup keeps the game,
    /// and a failed Windows list leaves every game installable (the Install box still explains).
    pub async fn fetch_library(&self, token: &Secret) -> Result<Vec<Game>, AuthError> {
        let items = self.items(token).await?;
        let windows_apps = self.windows_apps(token).await.ok();
        // Each lookup owns its data (client, token, ids) so the future is Send + 'static,
        // which Tauri's async commands require.
        let wanted: Vec<(String, String)> = items
            .iter()
            .filter(|i| worth_fetching(i))
            .map(|i| (i.namespace.clone(), i.catalog_item_id.clone()))
            .collect();
        let lookups: Vec<_> = wanted
            .into_iter()
            .map(|(namespace, id)| {
                let (client, token) = (self.clone(), token.clone());
                async move {
                    let info = client.catalog_item(&token, &namespace, &id).await.ok().flatten();
                    (id, info)
                }
            })
            .collect();
        let results: Vec<(String, Option<CatalogItem>)> = stream::iter(lookups).buffer_unordered(PARALLEL_LOOKUPS).collect().await;
        let catalog: HashMap<String, CatalogItem> = results.into_iter().filter_map(|(id, info)| info.map(|i| (id, i))).collect();
        Ok(build_library(&items, &catalog, windows_apps.as_ref()))
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
        let c = LibraryClient::with_base_urls(&server.uri(), &server.uri(), &server.uri());
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
        let c = LibraryClient::with_base_urls(&server.uri(), &server.uri(), &server.uri());
        let games = c.fetch_library(&Secret::new("eg1~A")).await.unwrap();
        assert_eq!(games.len(), 2);
        assert_eq!(games[0].title, "AppB"); // fallback title, sorted A-Z
        assert_eq!(games[1].title, "Tidepool Odyssey");
        assert_eq!(games[1].cover_url.as_deref(), Some("https://cdn/a.jpg"));
    }

    #[tokio::test]
    async fn fetch_library_marks_games_without_a_windows_build() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/library/api/public/items"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "records": [
                    {"namespace": "n1", "catalogItemId": "a", "appName": "AppA"},
                    {"namespace": "n2", "catalogItemId": "b", "appName": "AppB"}
                ],
                "responseMetadata": {}
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/launcher/api/public/assets/Windows"))
            .and(query_param("label", "Live"))
            .and(header("authorization", "bearer eg1~A"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"appName": "AppA", "namespace": "n1", "catalogItemId": "a", "labelName": "Live", "buildVersion": "1.0"}
            ])))
            .expect(1)
            .mount(&server)
            .await;
        let c = LibraryClient::with_base_urls(&server.uri(), &server.uri(), &server.uri());
        let games = c.fetch_library(&Secret::new("eg1~A")).await.unwrap();
        let flags: Vec<(&str, bool)> = games.iter().map(|g| (g.app_name.as_str(), g.installable)).collect();
        assert_eq!(flags, vec![("AppA", true), ("AppB", false)]);
    }

    #[tokio::test]
    async fn a_failed_windows_list_keeps_every_game_installable() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/library/api/public/items"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "records": [{"namespace": "n1", "catalogItemId": "a", "appName": "AppA"}],
                "responseMetadata": {}
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/launcher/api/public/assets/Windows"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;
        let c = LibraryClient::with_base_urls(&server.uri(), &server.uri(), &server.uri());
        let games = c.fetch_library(&Secret::new("eg1~A")).await.unwrap();
        assert!(games[0].installable);
    }

    #[tokio::test]
    async fn library_error_is_rejected() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({"errorCode": "errors.com.epicgames.common.authentication.token_verification_failed"})))
            .mount(&server)
            .await;
        let c = LibraryClient::with_base_urls(&server.uri(), &server.uri(), &server.uri());
        assert!(matches!(c.fetch_library(&Secret::new("x")).await, Err(AuthError::Rejected { .. })));
    }
}
