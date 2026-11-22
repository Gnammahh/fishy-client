//! The two Epic calls Play makes (spec §6d, Legendary `api/egs.py` `get_game_token` / `get_ownership_token`).
use crate::auth::client::{http_client, network, read_json, EPIC_ACCOUNT_HOST};
use crate::auth::{AuthError, Secret};
use reqwest::Client;

pub const ECOMMERCE_HOST: &str = "https://ecommerceintegration-public-service-ecomprod02.ol.epicgames.com";

#[derive(Clone)]
pub struct LaunchClient {
    http: Client,
    account_base: String,
    ecommerce_base: String,
    retry_base: std::time::Duration,
}

impl Default for LaunchClient {
    fn default() -> Self {
        Self::new()
    }
}

impl LaunchClient {
    pub fn new() -> Self {
        Self::with_base_urls(EPIC_ACCOUNT_HOST, ECOMMERCE_HOST)
    }

    /// For tests: point both services at a fake Epic server.
    pub fn with_base_urls(account: &str, ecommerce: &str) -> Self {
        Self {
            http: http_client(),
            account_base: account.trim_end_matches('/').to_string(),
            ecommerce_base: ecommerce.trim_end_matches('/').to_string(),
            retry_base: crate::retry::RETRY_BASE,
        }
    }

    /// For tests: shorter waits between retries.
    pub fn with_retry_base(mut self, d: std::time::Duration) -> Self {
        self.retry_base = d;
        self
    }

    /// A fresh one-time code for `-AUTH_PASSWORD` (valid about 5 minutes).
    pub async fn exchange_code(&self, token: &Secret) -> Result<Secret, AuthError> {
        let json = crate::retry::with_retries("launch code", self.retry_base, || async {
            let res = self
                .http
                .get(format!("{}/account/api/oauth/exchange", self.account_base))
                .header("Authorization", format!("bearer {}", token.expose()))
                .send()
                .await
                .map_err(network)?;
            read_json(res).await
        })
        .await?;
        let code = json.get("code").and_then(|c| c.as_str()).filter(|c| !c.is_empty());
        code.map(Secret::new).ok_or_else(|| AuthError::Unexpected("exchange reply has no code".into()))
    }

    /// The ownership token file's bytes, for games that need `-epicovt`.
    pub async fn ownership_token(&self, token: &Secret, account_id: &str, namespace: &str, catalog_item_id: &str) -> Result<Vec<u8>, AuthError> {
        crate::retry::with_retries("ownership token", self.retry_base, || self.ownership_token_once(token, account_id, namespace, catalog_item_id)).await
    }

    async fn ownership_token_once(&self, token: &Secret, account_id: &str, namespace: &str, catalog_item_id: &str) -> Result<Vec<u8>, AuthError> {
        let res = self
            .http
            .post(format!("{}/ecommerceintegration/api/public/platforms/EPIC/identities/{account_id}/ownershipToken", self.ecommerce_base))
            .header("Authorization", format!("bearer {}", token.expose()))
            .form(&[("nsCatalogItemId", format!("{namespace}:{catalog_item_id}"))])
            .send()
            .await
            .map_err(network)?;
        if !res.status().is_success() {
            // Reuse the shared reader for its errorCode handling; it never keeps Epic's message.
            return Err(read_json(res).await.err().unwrap_or(AuthError::Unexpected("ownership token refused".into())));
        }
        Ok(res.bytes().await.map_err(network)?.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_string, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn client() -> (MockServer, LaunchClient) {
        let server = MockServer::start().await;
        let c = LaunchClient::with_base_urls(&server.uri(), &server.uri()).with_retry_base(std::time::Duration::from_millis(1));
        (server, c)
    }

    fn dead() -> LaunchClient {
        LaunchClient::with_base_urls("http://127.0.0.1:9", "http://127.0.0.1:9").with_retry_base(std::time::Duration::from_millis(1))
    }

    #[tokio::test]
    async fn exchange_code_and_ownership_token_retry_after_server_errors() {
        let (server, c) = client().await;
        let c = c.with_retry_base(std::time::Duration::from_millis(1));
        Mock::given(method("GET")).and(path("/account/api/oauth/exchange")).respond_with(ResponseTemplate::new(503)).up_to_n_times(2).mount(&server).await;
        Mock::given(method("GET"))
            .and(path("/account/api/oauth/exchange"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"code": "abc123"})))
            .mount(&server)
            .await;
        assert_eq!(c.exchange_code(&Secret::new("eg1~A")).await.unwrap().expose(), "abc123");
        let ovt = "/ecommerceintegration/api/public/platforms/EPIC/identities/acc/ownershipToken";
        Mock::given(method("POST")).and(path(ovt)).respond_with(ResponseTemplate::new(503)).up_to_n_times(2).mount(&server).await;
        Mock::given(method("POST")).and(path(ovt)).respond_with(ResponseTemplate::new(200).set_body_bytes(b"token".to_vec())).mount(&server).await;
        assert_eq!(c.ownership_token(&Secret::new("eg1~A"), "acc", "ns", "cat").await.unwrap(), b"token".to_vec());
    }

    #[tokio::test]
    async fn exchange_code_returns_the_code() {
        let (server, c) = client().await;
        Mock::given(method("GET"))
            .and(path("/account/api/oauth/exchange"))
            .and(header("Authorization", "bearer eg1~A"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"expiresInSeconds": 300, "code": "abc123", "creatorId": "acc123"})))
            .expect(1)
            .mount(&server)
            .await;
        assert_eq!(c.exchange_code(&Secret::new("eg1~A")).await.unwrap().expose(), "abc123");
    }

    #[tokio::test]
    async fn exchange_code_rejected_and_offline() {
        let (server, c) = client().await;
        Mock::given(method("GET"))
            .and(path("/account/api/oauth/exchange"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({"errorCode": "errors.com.epicgames.common.oauth.invalid_token"})))
            .mount(&server)
            .await;
        assert_eq!(
            c.exchange_code(&Secret::new("eg1~A")).await,
            Err(AuthError::Rejected { code: "errors.com.epicgames.common.oauth.invalid_token".into() })
        );
        assert!(matches!(dead().exchange_code(&Secret::new("eg1~A")).await, Err(AuthError::Network(_))));
    }

    #[tokio::test]
    async fn exchange_code_without_code_is_unexpected() {
        let (server, c) = client().await;
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({}))).mount(&server).await;
        assert!(matches!(c.exchange_code(&Secret::new("eg1~A")).await, Err(AuthError::Unexpected(_))));
    }

    #[tokio::test]
    async fn ownership_token_posts_the_item_and_returns_raw_bytes() {
        let (server, c) = client().await;
        Mock::given(method("POST"))
            .and(path("/ecommerceintegration/api/public/platforms/EPIC/identities/acc123/ownershipToken"))
            .and(header("Authorization", "bearer eg1~A"))
            .and(body_string("nsCatalogItemId=ns1%3Acat1"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![1u8, 2, 3, 0, 255]))
            .expect(1)
            .mount(&server)
            .await;
        assert_eq!(c.ownership_token(&Secret::new("eg1~A"), "acc123", "ns1", "cat1").await.unwrap(), vec![1u8, 2, 3, 0, 255]);
    }

    #[tokio::test]
    async fn ownership_token_error_body_is_rejected_not_saved() {
        let (server, c) = client().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({"errorCode": "errors.com.epicgames.ecommerce.not_owned"})))
            .mount(&server)
            .await;
        assert_eq!(
            c.ownership_token(&Secret::new("eg1~A"), "acc123", "ns1", "cat1").await,
            Err(AuthError::Rejected { code: "errors.com.epicgames.ecommerce.not_owned".into() })
        );
        assert!(matches!(dead().ownership_token(&Secret::new("eg1~A"), "a", "n", "c").await, Err(AuthError::Network(_))));
    }
}
