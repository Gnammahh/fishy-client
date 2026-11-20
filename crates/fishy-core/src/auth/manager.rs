use super::{AuthError, EpicClient, Session, SessionStore};
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

/// What the UI needs to know about the login. Tagged by `kind` for the frontend.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind")]
pub enum AuthState {
    SignedOut,
    SignedIn { display_name: String, account_id: String },
    /// A saved session exists but Epic couldn't be reached to refresh it.
    Offline { display_name: String },
}

/// Refresh this long before the access token expires (spec §6a).
const REFRESH_MARGIN_MINUTES: i64 = 5;

/// Owns the login rules: when to refresh, when to sign out, what to keep.
pub struct AuthManager<S: SessionStore> {
    client: EpicClient,
    store: S,
    now: fn() -> DateTime<Utc>,
}

fn signed_in(s: &Session) -> AuthState {
    AuthState::SignedIn { display_name: s.display_name.clone(), account_id: s.account_id.clone() }
}

impl<S: SessionStore> AuthManager<S> {
    pub fn new(client: EpicClient, store: S, now: fn() -> DateTime<Utc>) -> Self {
        Self { client, store, now }
    }

    pub fn store(&self) -> &S {
        &self.store
    }

    /// Run once when the app starts.
    pub async fn startup(&self) -> AuthState {
        let session = match self.store.load() {
            Ok(Some(s)) => s,
            Ok(None) => return AuthState::SignedOut,
            Err(_) => {
                let _ = self.store.clear(); // damaged entry: start fresh
                return AuthState::SignedOut;
            }
        };
        let now = (self.now)();
        if session.refresh_expires_at <= now {
            let _ = self.store.clear();
            return AuthState::SignedOut;
        }
        if session.expires_at - now > Duration::minutes(REFRESH_MARGIN_MINUTES) {
            return signed_in(&session);
        }
        match self.client.refresh(&session.refresh_token).await {
            Ok(fresh) => {
                let _ = self.store.save(&fresh);
                signed_in(&fresh)
            }
            Err(AuthError::Network(_)) => AuthState::Offline { display_name: session.display_name },
            Err(_) => {
                let _ = self.store.clear();
                AuthState::SignedOut
            }
        }
    }

    /// Trade the authorization code from the login window for a saved session.
    pub async fn complete_login(&self, code: &str) -> Result<AuthState, AuthError> {
        let session = self.client.exchange_code(code).await?;
        self.store
            .save(&session)
            .map_err(|_| AuthError::Unexpected("Couldn't save the sign-in to Windows Credential Manager.".into()))?;
        Ok(signed_in(&session))
    }

    /// End the session at Epic (best effort) and always forget it locally.
    pub async fn logout(&self) -> AuthState {
        if let Ok(Some(session)) = self.store.load() {
            let _ = self.client.kill_session(&session.access_token).await;
        }
        let _ = self.store.clear();
        AuthState::SignedOut
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{EpicClient, MemoryStore, Secret, Session};
    use chrono::{DateTime, Duration, TimeZone, Utc};
    use wiremock::matchers::{body_string_contains, method};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 20, 18, 0, 0).unwrap()
    }

    fn session(access_in: Duration, refresh_in: Duration) -> Session {
        Session {
            access_token: Secret::new("eg1~A"),
            refresh_token: Secret::new("eg1~R"),
            expires_at: now() + access_in,
            refresh_expires_at: now() + refresh_in,
            account_id: "acc123".into(),
            display_name: "FishFan".into(),
        }
    }

    fn fresh_token_json() -> serde_json::Value {
        serde_json::json!({
            "access_token": "eg1~NEW", "expires_at": "2026-11-20T20:00:00.000Z",
            "refresh_token": "eg1~NEWR", "refresh_expires_at": "2026-12-13T18:00:00.000Z",
            "account_id": "acc123", "displayName": "FishFan"
        })
    }

    fn manager(server: &str, store: MemoryStore) -> AuthManager<MemoryStore> {
        AuthManager::new(EpicClient::with_base_url(server), store, now)
    }

    fn signed_in() -> AuthState {
        AuthState::SignedIn { display_name: "FishFan".into(), account_id: "acc123".into() }
    }

    #[tokio::test]
    async fn startup_without_session_is_signed_out() {
        let server = MockServer::start().await;
        let m = manager(&server.uri(), MemoryStore::default());
        assert_eq!(m.startup().await, AuthState::SignedOut);
    }

    #[tokio::test]
    async fn startup_with_fresh_session_skips_refresh() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).respond_with(ResponseTemplate::new(500)).expect(0).mount(&server).await;
        let store = MemoryStore::with(session(Duration::hours(2), Duration::days(20)));
        assert_eq!(manager(&server.uri(), store).startup().await, signed_in());
    }

    #[tokio::test]
    async fn startup_refreshes_when_expiring_soon() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(body_string_contains("grant_type=refresh_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(fresh_token_json()))
            .expect(1)
            .mount(&server)
            .await;
        let store = MemoryStore::with(session(Duration::minutes(4), Duration::days(20)));
        let m = manager(&server.uri(), store);
        assert_eq!(m.startup().await, signed_in());
        assert_eq!(m.store().load().unwrap().unwrap().access_token.expose(), "eg1~NEW");
    }

    #[tokio::test]
    async fn startup_with_dead_refresh_token_clears_without_network() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).respond_with(ResponseTemplate::new(200)).expect(0).mount(&server).await;
        let store = MemoryStore::with(session(-Duration::hours(1), -Duration::minutes(1)));
        let m = manager(&server.uri(), store);
        assert_eq!(m.startup().await, AuthState::SignedOut);
        assert!(m.store().load().unwrap().is_none());
    }

    #[tokio::test]
    async fn startup_rejected_refresh_signs_out_and_clears() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({"errorCode": "errors.com.epicgames.account.auth_token.invalid_refresh_token", "errorMessage": "nope"})))
            .mount(&server)
            .await;
        let store = MemoryStore::with(session(Duration::minutes(1), Duration::days(20)));
        let m = manager(&server.uri(), store);
        assert_eq!(m.startup().await, AuthState::SignedOut);
        assert!(m.store().load().unwrap().is_none());
    }

    #[tokio::test]
    async fn startup_network_error_is_offline_and_keeps_session() {
        let store = MemoryStore::with(session(Duration::minutes(1), Duration::days(20)));
        let m = manager("http://127.0.0.1:9", store);
        assert_eq!(m.startup().await, AuthState::Offline { display_name: "FishFan".into() });
        assert!(m.store().load().unwrap().is_some());
    }

    #[tokio::test]
    async fn complete_login_saves_session() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(body_string_contains("grant_type=authorization_code"))
            .respond_with(ResponseTemplate::new(200).set_body_json(fresh_token_json()))
            .mount(&server)
            .await;
        let m = manager(&server.uri(), MemoryStore::default());
        assert_eq!(m.complete_login("abc").await.unwrap(), signed_in());
        assert_eq!(m.store().load().unwrap().unwrap().refresh_token.expose(), "eg1~NEWR");
    }

    #[tokio::test]
    async fn logout_clears_even_if_kill_fails() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE")).respond_with(ResponseTemplate::new(500)).expect(1).mount(&server).await;
        let store = MemoryStore::with(session(Duration::hours(2), Duration::days(20)));
        let m = manager(&server.uri(), store);
        assert_eq!(m.logout().await, AuthState::SignedOut);
        assert!(m.store().load().unwrap().is_none());
    }
}
