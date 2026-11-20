use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

/// A token value that never shows up in logs: Debug and Display print `[redacted]`.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    /// The real value. Only call this where the token is actually sent to Epic.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted]")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted]")
    }
}

/// A signed-in Epic session, as returned by the OAuth token endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub access_token: Secret,
    pub refresh_token: Secret,
    pub expires_at: DateTime<Utc>,
    pub refresh_expires_at: DateTime<Utc>,
    pub account_id: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) const TOKEN_JSON: &str = r#"{
        "access_token": "eg1~ACCESS-SECRET-VALUE",
        "expires_in": 7200,
        "expires_at": "2026-11-20T20:00:00.000Z",
        "token_type": "bearer",
        "refresh_token": "eg1~REFRESH-SECRET-VALUE",
        "refresh_expires": 1987200,
        "refresh_expires_at": "2026-12-13T18:00:00.000Z",
        "account_id": "acc123",
        "client_id": "34a02cf8f4414e29b15921876da36f9a",
        "internal_client": true,
        "client_service": "launcher",
        "displayName": "FishFan",
        "app": "launcher",
        "in_app_id": "acc123"
    }"#;

    #[test]
    fn session_parses_epic_token_json() {
        let s: Session = serde_json::from_str(TOKEN_JSON).unwrap();
        assert_eq!(s.access_token.expose(), "eg1~ACCESS-SECRET-VALUE");
        assert_eq!(s.refresh_token.expose(), "eg1~REFRESH-SECRET-VALUE");
        assert_eq!(s.account_id, "acc123");
        assert_eq!(s.display_name, "FishFan");
        assert_eq!(s.expires_at.to_rfc3339(), "2026-11-20T20:00:00+00:00");
        assert_eq!(s.refresh_expires_at.to_rfc3339(), "2026-12-13T18:00:00+00:00");
    }

    #[test]
    fn secret_is_redacted_in_debug() {
        let s: Session = serde_json::from_str(TOKEN_JSON).unwrap();
        let dbg = format!("{s:?}");
        assert!(!dbg.contains("ACCESS-SECRET-VALUE"));
        assert!(!dbg.contains("REFRESH-SECRET-VALUE"));
        assert!(dbg.contains("[redacted]"));
        assert_eq!(format!("{}", s.access_token), "[redacted]");
    }

    #[test]
    fn session_round_trips_json() {
        let s: Session = serde_json::from_str(TOKEN_JSON).unwrap();
        let again: Session = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(again, s);
    }
}
