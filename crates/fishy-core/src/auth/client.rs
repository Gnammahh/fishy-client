use super::{AuthError, Secret, Session};
use reqwest::{Client, Response};
use serde_json::Value;

use crate::epic::{ACCOUNT_HOST, ACCOUNT_USER_AGENT, CLIENT_ID, CLIENT_SECRET};

/// HTTP client with the launcher User-Agent and a 20 s timeout, shared by all Epic calls.
pub(crate) fn http_client() -> Client {
    Client::builder()
        .user_agent(ACCOUNT_USER_AGENT)
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .expect("http client")
}

/// Talks to Epic's account service. All requests carry the launcher User-Agent.
/// One sign-in refresh try gives up after this, so a network that goes silent can't hold
/// Log out, Play or Try again for a minute (spec §6f.2: 3 tries).
pub const REFRESH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);

#[derive(Clone)]
pub struct EpicClient {
    http: Client,
    base: String,
    retry_base: std::time::Duration,
    refresh_timeout: std::time::Duration,
}

impl Default for EpicClient {
    fn default() -> Self {
        Self::new()
    }
}

impl EpicClient {
    pub fn new() -> Self {
        Self::with_base_url(ACCOUNT_HOST)
    }

    /// For tests: point the client at a fake Epic server.
    pub fn with_base_url(base: &str) -> Self {
        Self { http: http_client(), base: base.trim_end_matches('/').to_string(), retry_base: crate::retry::RETRY_BASE, refresh_timeout: REFRESH_TIMEOUT }
    }

    /// For tests: shorter waits between retries.
    pub fn with_retry_base(mut self, d: std::time::Duration) -> Self {
        self.retry_base = d;
        self
    }

    /// For tests: a shorter cap on each refresh try.
    pub fn with_refresh_timeout(mut self, d: std::time::Duration) -> Self {
        self.refresh_timeout = d;
        self
    }

    pub async fn exchange_code(&self, code: &str) -> Result<Session, AuthError> {
        self.token(&[("grant_type", "authorization_code"), ("code", code), ("token_type", "eg1")], None).await
    }

    /// Retried after connection problems (spec §6f.2). The one-time code exchange above never is.
    pub async fn refresh(&self, refresh_token: &Secret) -> Result<Session, AuthError> {
        let form = [("grant_type", "refresh_token"), ("refresh_token", refresh_token.expose()), ("token_type", "eg1")];
        crate::retry::with_retries("sign-in refresh", self.retry_base, || self.token(&form, Some(self.refresh_timeout))).await
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

    /// `timeout`: a cap for this one request instead of the client's 20 s.
    async fn token(&self, form: &[(&str, &str)], timeout: Option<std::time::Duration>) -> Result<Session, AuthError> {
        let mut req = self.http.post(format!("{}/account/api/oauth/token", self.base)).basic_auth(CLIENT_ID, Some(CLIENT_SECRET)).form(form);
        if let Some(t) = timeout {
            req = req.timeout(t);
        }
        let res = req
            .send()
            .await
            .map_err(network)?;
        let json = read_json(res).await?;
        serde_json::from_value(json).map_err(|e| AuthError::Unexpected(e.to_string()))
    }
}

/// Never keep the URL: the kill endpoint has the access token in its path.
pub(crate) fn network(e: reqwest::Error) -> AuthError {
    AuthError::Network(e.without_url().to_string())
}

/// Turns any non-2xx or `errorCode` body into `Rejected`, except Epic's own server errors (5xx),
/// "too many requests" (429) and "request timeout" (408), which count as a connection problem
/// (retried, and never "your sign-in was refused").
/// Empty 2xx bodies become `Value::Null`.
pub(crate) async fn read_json(res: Response) -> Result<Value, AuthError> {
    let status = res.status();
    if status.is_server_error() || status.as_u16() == 408 || status.as_u16() == 429 {
        return Err(AuthError::Network(format!("http_{}", status.as_u16())));
    }
    let text = res.text().await.map_err(network)?;
    let json: Value = if text.trim().is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&text).unwrap_or(Value::Null)
    };
    let error_code = json.get("errorCode").and_then(Value::as_str);
    if error_code.is_some() || !status.is_success() {
        // errorMessage is deliberately not kept: Epic sometimes quotes the token in it.
        return Err(AuthError::Rejected {
            code: error_code.map(str::to_string).unwrap_or_else(|| format!("http_{}", status.as_u16())),
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
        let s = EpicClient::with_base_url(&server.uri()).with_retry_base(std::time::Duration::from_millis(1)).exchange_code("abc").await.unwrap();
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
        let s = EpicClient::with_base_url(&server.uri()).with_retry_base(std::time::Duration::from_millis(1)).refresh(&Secret::new("eg1~OLD")).await.unwrap();
        assert_eq!(s.refresh_token.expose(), "eg1~R");
    }

    #[tokio::test]
    async fn refresh_retries_after_server_errors() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).respond_with(ResponseTemplate::new(503)).up_to_n_times(2).mount(&server).await;
        Mock::given(method("POST")).respond_with(ResponseTemplate::new(200).set_body_json(token_json())).expect(1).mount(&server).await;
        let c = EpicClient::with_base_url(&server.uri()).with_retry_base(std::time::Duration::from_millis(1));
        assert_eq!(c.refresh(&Secret::new("eg1~OLD")).await.unwrap().display_name, "FishFan");
    }

    #[tokio::test]
    async fn a_silent_network_gives_up_quickly_on_refresh() {
        // Epic never answers: three short tries, not three 20-second waits.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(token_json()).set_delay(std::time::Duration::from_secs(5)))
            .mount(&server)
            .await;
        let c = EpicClient::with_base_url(&server.uri()).with_retry_base(std::time::Duration::from_millis(1)).with_refresh_timeout(std::time::Duration::from_millis(200));
        let started = std::time::Instant::now();
        assert!(matches!(c.refresh(&Secret::new("eg1~OLD")).await, Err(AuthError::Network(_))));
        assert!(started.elapsed() < std::time::Duration::from_secs(2), "{:?}", started.elapsed());
    }

    #[test]
    fn refresh_tries_are_capped_at_8_seconds() {
        assert_eq!(REFRESH_TIMEOUT, std::time::Duration::from_secs(8));
    }

    #[tokio::test]
    async fn exchange_code_is_tried_once() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).respond_with(ResponseTemplate::new(503)).expect(1).mount(&server).await;
        let c = EpicClient::with_base_url(&server.uri()).with_retry_base(std::time::Duration::from_millis(1));
        assert!(matches!(c.exchange_code("abc").await, Err(AuthError::Network(_))));
    }

    #[tokio::test]
    async fn too_many_requests_and_timeouts_count_as_network() {
        for status in [408, 429] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(status).set_body_json(serde_json::json!({"errorCode": "errors.com.epicgames.common.throttled"})))
                .mount(&server)
                .await;
            let c = EpicClient::with_base_url(&server.uri()).with_retry_base(std::time::Duration::from_millis(1));
            assert!(matches!(c.refresh(&Secret::new("x")).await, Err(AuthError::Network(_))), "{status}");
        }
    }

    #[tokio::test]
    async fn server_errors_count_as_network() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(503).set_body_json(serde_json::json!({
                "errorCode": "errors.com.epicgames.common.server_error"
            })))
            .mount(&server)
            .await;
        let e = EpicClient::with_base_url(&server.uri()).with_retry_base(std::time::Duration::from_millis(1)).exchange_code("abc").await.unwrap_err();
        assert!(matches!(e, AuthError::Network(_)), "{e:?}");
    }

    #[tokio::test]
    async fn a_401_with_an_error_code_is_still_rejected() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "errorCode": "errors.com.epicgames.account.auth_token.invalid_refresh_token"
            })))
            .mount(&server)
            .await;
        let e = EpicClient::with_base_url(&server.uri()).with_retry_base(std::time::Duration::from_millis(1)).refresh(&Secret::new("eg1~OLD")).await.unwrap_err();
        assert!(matches!(e, AuthError::Rejected { .. }), "{e:?}");
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
        let err = EpicClient::with_base_url(&server.uri()).with_retry_base(std::time::Duration::from_millis(1)).exchange_code("abc").await.unwrap_err();
        assert_eq!(
            err,
            AuthError::Rejected { code: "errors.com.epicgames.account.oauth.authorization_code_not_found".into() }
        );
    }

    #[tokio::test]
    async fn rejected_error_never_keeps_epics_message() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "errorCode": "errors.com.epicgames.account.auth_token.invalid_refresh_token",
                "errorMessage": "Sorry the refresh token 'eg1~SECRET-REFRESH' is invalid"
            })))
            .mount(&server)
            .await;
        let err = EpicClient::with_base_url(&server.uri()).with_retry_base(std::time::Duration::from_millis(1)).refresh(&Secret::new("eg1~SECRET-REFRESH")).await.unwrap_err();
        assert!(!format!("{err:?}").contains("SECRET-REFRESH"));
        assert!(!err.to_string().contains("SECRET-REFRESH"));
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
        let err = EpicClient::with_base_url(&server.uri()).with_retry_base(std::time::Duration::from_millis(1)).refresh(&Secret::new("x")).await.unwrap_err();
        assert!(matches!(err, AuthError::Rejected { .. }));
    }

    #[tokio::test]
    async fn connection_refused_is_network() {
        let err = EpicClient::with_base_url("http://127.0.0.1:9").with_retry_base(std::time::Duration::from_millis(1)).exchange_code("abc").await.unwrap_err();
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
        EpicClient::with_base_url(&server.uri()).with_retry_base(std::time::Duration::from_millis(1)).kill_session(&Secret::new("eg1~A")).await.unwrap();
    }

    #[test]
    fn errors_never_print_tokens() {
        let e = AuthError::Network("connection reset".into());
        assert_eq!(e.to_string(), "Couldn't reach Epic. Check your internet connection.");
    }
}
