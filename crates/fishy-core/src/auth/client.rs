use super::{AuthError, Secret, Session};
use reqwest::{Client, Response};
use serde_json::Value;

pub const EPIC_ACCOUNT_HOST: &str = "https://account-public-service-prod03.ol.epicgames.com";
/// The Epic Games Launcher's public OAuth client (same as Legendary/Heroic).
pub const CLIENT_ID: &str = "34a02cf8f4414e29b15921876da36f9a";
const CLIENT_SECRET: &str = "daafbccc737745039dffe53d94fc76cf";
const USER_AGENT: &str = "UELauncher/11.0.1-14907503+++Portal+Release-Live Windows/10.0.19041.1.256.64bit";

/// Talks to Epic's account service. All requests carry the launcher User-Agent.
#[derive(Clone)]
pub struct EpicClient {
    http: Client,
    base: String,
}

impl Default for EpicClient {
    fn default() -> Self {
        Self::new()
    }
}

impl EpicClient {
    pub fn new() -> Self {
        Self::with_base_url(EPIC_ACCOUNT_HOST)
    }

    /// For tests: point the client at a fake Epic server.
    pub fn with_base_url(base: &str) -> Self {
        let http = Client::builder()
            .user_agent(USER_AGENT)
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .expect("http client");
        Self { http, base: base.trim_end_matches('/').to_string() }
    }

    pub async fn exchange_code(&self, code: &str) -> Result<Session, AuthError> {
        self.token(&[("grant_type", "authorization_code"), ("code", code), ("token_type", "eg1")]).await
    }

    pub async fn refresh(&self, refresh_token: &Secret) -> Result<Session, AuthError> {
        self.token(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token.expose()),
            ("token_type", "eg1"),
        ])
        .await
    }

    /// Ends the session on Epic's side.
    pub async fn kill_session(&self, access_token: &Secret) -> Result<(), AuthError> {
        let url = format!("{}/account/api/oauth/sessions/kill/{}", self.base, access_token.expose());
        let res = self
            .http
            .delete(url)
            .header("Authorization", format!("bearer {}", access_token.expose()))
            .send()
            .await
            .map_err(network)?;
        read_json(res).await.map(|_| ())
    }

    async fn token(&self, form: &[(&str, &str)]) -> Result<Session, AuthError> {
        let res = self
            .http
            .post(format!("{}/account/api/oauth/token", self.base))
            .basic_auth(CLIENT_ID, Some(CLIENT_SECRET))
            .form(form)
            .send()
            .await
            .map_err(network)?;
        let json = read_json(res).await?;
        serde_json::from_value(json).map_err(|e| AuthError::Unexpected(e.to_string()))
    }
}

/// Never keep the URL: the kill endpoint has the access token in its path.
fn network(e: reqwest::Error) -> AuthError {
    AuthError::Network(e.without_url().to_string())
}

/// Turns any non-2xx or `errorCode` body into `Rejected`. Empty 2xx bodies become `Value::Null`.
async fn read_json(res: Response) -> Result<Value, AuthError> {
    let status = res.status();
    let text = res.text().await.map_err(network)?;
    let json: Value = if text.trim().is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&text).unwrap_or(Value::Null)
    };
    let error_code = json.get("errorCode").and_then(Value::as_str);
    if error_code.is_some() || !status.is_success() {
        return Err(AuthError::Rejected {
            code: error_code.map(str::to_string).unwrap_or_else(|| format!("http_{}", status.as_u16())),
            message: json.get("errorMessage").and_then(Value::as_str).unwrap_or_default().to_string(),
        });
    }
    Ok(json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_string_contains, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const BASIC: &str = "Basic MzRhMDJjZjhmNDQxNGUyOWIxNTkyMTg3NmRhMzZmOWE6ZGFhZmJjY2M3Mzc3NDUwMzlkZmZlNTNkOTRmYzc2Y2Y=";
    const UA: &str = "UELauncher/11.0.1-14907503+++Portal+Release-Live Windows/10.0.19041.1.256.64bit";

    fn token_json() -> serde_json::Value {
        serde_json::json!({
            "access_token": "eg1~A", "expires_in": 7200, "expires_at": "2026-11-20T20:00:00.000Z",
            "token_type": "bearer", "refresh_token": "eg1~R", "refresh_expires": 1987200,
            "refresh_expires_at": "2026-12-13T18:00:00.000Z", "account_id": "acc123",
            "client_id": "34a02cf8f4414e29b15921876da36f9a", "displayName": "FishFan"
        })
    }

    #[tokio::test]
    async fn exchange_code_posts_form_with_basic_auth() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/account/api/oauth/token"))
            .and(header("authorization", BASIC))
            .and(header("user-agent", UA))
            .and(body_string_contains("grant_type=authorization_code"))
            .and(body_string_contains("code=abc"))
            .and(body_string_contains("token_type=eg1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(token_json()))
            .expect(1)
            .mount(&server)
            .await;
        let s = EpicClient::with_base_url(&server.uri()).exchange_code("abc").await.unwrap();
        assert_eq!(s.display_name, "FishFan");
        assert_eq!(s.access_token.expose(), "eg1~A");
    }

    #[tokio::test]
    async fn refresh_sends_refresh_grant() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/account/api/oauth/token"))
            .and(header("authorization", BASIC))
            .and(body_string_contains("grant_type=refresh_token"))
            .and(body_string_contains("refresh_token=eg1%7EOLD"))
            .and(body_string_contains("token_type=eg1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(token_json()))
            .expect(1)
            .mount(&server)
            .await;
        let s = EpicClient::with_base_url(&server.uri()).refresh(&Secret::new("eg1~OLD")).await.unwrap();
        assert_eq!(s.refresh_token.expose(), "eg1~R");
    }

    #[tokio::test]
    async fn error_json_is_rejected() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "errorCode": "errors.com.epicgames.account.oauth.authorization_code_not_found",
                "errorMessage": "Sorry the authorization code you supplied was not found."
            })))
            .mount(&server)
            .await;
        let err = EpicClient::with_base_url(&server.uri()).exchange_code("abc").await.unwrap_err();
        assert_eq!(
            err,
            AuthError::Rejected {
                code: "errors.com.epicgames.account.oauth.authorization_code_not_found".into(),
                message: "Sorry the authorization code you supplied was not found.".into(),
            }
        );
    }

    #[tokio::test]
    async fn error_json_with_200_is_rejected() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"errorCode": "errors.com.epicgames.common.oauth.invalid_token", "errorMessage": "bad"})),
            )
            .mount(&server)
            .await;
        let err = EpicClient::with_base_url(&server.uri()).refresh(&Secret::new("x")).await.unwrap_err();
        assert!(matches!(err, AuthError::Rejected { .. }));
    }

    #[tokio::test]
    async fn connection_refused_is_network() {
        let err = EpicClient::with_base_url("http://127.0.0.1:9").exchange_code("abc").await.unwrap_err();
        assert!(matches!(err, AuthError::Network(_)));
    }

    #[tokio::test]
    async fn kill_session_sends_delete_with_bearer() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/account/api/oauth/sessions/kill/eg1~A"))
            .and(header("authorization", "bearer eg1~A"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
        EpicClient::with_base_url(&server.uri()).kill_session(&Secret::new("eg1~A")).await.unwrap();
    }

    #[test]
    fn errors_never_print_tokens() {
        let e = AuthError::Network("connection reset".into());
        assert_eq!(e.to_string(), "Couldn't reach Epic. Check your internet connection.");
    }
}
