use url::Url;

/// Where the login window's helper script sends the code; caught and cancelled in Rust.
pub const CAPTURE_PREFIX: &str = "http://fishy-auth.localhost/done";

fn is_code(s: &str) -> bool {
    s.len() == 32 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// `code=<32 hex>` from a URL like Epic's `redirectUrl` (`…/authorized?code=…`).
fn code_in_url(s: &str) -> Option<String> {
    let url = Url::parse(s.trim()).ok()?;
    let (_, v) = url.query_pairs().find(|(k, _)| k == "code")?;
    is_code(&v).then(|| v.into_owned())
}

/// Accepts a bare authorization code, a quoted one, Epic's whole redirect JSON,
/// or the `redirectUrl` from it. Anything that isn't a real 32-hex code is refused.
pub fn parse_auth_code(input: &str) -> Option<String> {
    let trimmed = input.trim().trim_matches(|c| c == '"' || c == '\'').trim();
    if is_code(trimmed) {
        return Some(trimmed.to_string());
    }
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(input.trim()) {
        if let Some(code) = json.get("authorizationCode").and_then(|v| v.as_str()).map(str::trim)
            && is_code(code) {
                return Some(code.to_string());
            }
        return json.get("redirectUrl").and_then(|v| v.as_str()).and_then(code_in_url);
    }
    code_in_url(trimmed)
}

/// What the login window should do with a navigation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Navigation {
    /// A normal page: let it load.
    Allow,
    /// Our capture address, coming straight from Epic's redirect page: cancel it and use the code.
    Capture(String),
    /// Our capture address from anywhere else, or without a real code: cancel and ignore.
    Block,
}

/// Decides navigations in the login window. A code is only trusted when the previous page
/// was Epic's own `/id/api/redirect`, so no other page (ads, social logins, redirects)
/// can sign the user into a different account.
#[derive(Debug, Default)]
pub struct CaptureGuard {
    last_allowed: Option<String>,
}

fn is_epic_redirect_page(url: &str) -> bool {
    Url::parse(url)
        .map(|u| u.scheme() == "https" && u.host_str() == Some(crate::epic::LOGIN_HOST) && u.path() == crate::epic::REDIRECT_PATH)
        .unwrap_or(false)
}

impl CaptureGuard {
    pub fn on_navigate(&mut self, url: &str) -> Navigation {
        let is_capture = Url::parse(url)
            .map(|u| u.host_str() == Some("fishy-auth.localhost"))
            .unwrap_or(false);
        if !is_capture {
            self.last_allowed = Some(url.to_string());
            return Navigation::Allow;
        }
        let from_epic = self.last_allowed.as_deref().map(is_epic_redirect_page).unwrap_or(false);
        match code_from_capture_url(url) {
            Some(code) if from_epic && is_code(&code) => Navigation::Capture(code),
            _ => Navigation::Block,
        }
    }
}

/// The `code` from `http://fishy-auth.localhost/done?code=…`, and nothing from any other address.
pub fn code_from_capture_url(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    if parsed.scheme() != "http" || parsed.host_str() != Some("fishy-auth.localhost") || parsed.path() != "/done" {
        return None;
    }
    parsed
        .query_pairs()
        .find(|(k, _)| k == "code")
        .map(|(_, v)| v.into_owned())
        .filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CODE: &str = "a1b2c3d4e5f60718293a4b5c6d7e8f90";

    #[test]
    fn parse_bare_code() {
        assert_eq!(parse_auth_code(&format!("  {CODE}\n")), Some(CODE.to_string()));
    }

    #[test]
    fn parse_quoted_code() {
        assert_eq!(parse_auth_code(&format!("\"{CODE}\"")), Some(CODE.to_string()));
    }

    #[test]
    fn parse_full_redirect_json() {
        let json = format!(
            r#"{{"warning":"Do not share this code with any 3rd party service.","redirectUrl":"https://localhost/launcher/authorized?code={CODE}","authorizationCode":"{CODE}","exchangeCode":null,"sid":null}}"#
        );
        assert_eq!(parse_auth_code(&json), Some(CODE.to_string()));
    }

    #[test]
    fn parse_rejects_null_code_and_junk() {
        assert_eq!(parse_auth_code(r#"{"authorizationCode":null}"#), None);
        assert_eq!(parse_auth_code("hello"), None);
        assert_eq!(parse_auth_code(""), None);
    }

    #[test]
    fn capture_url_extracts_code() {
        assert_eq!(
            code_from_capture_url(&format!("http://fishy-auth.localhost/done?code={CODE}")),
            Some(CODE.to_string())
        );
    }

    #[test]
    fn capture_url_ignores_other_hosts() {
        assert_eq!(code_from_capture_url("https://www.epicgames.com/id/api/redirect?code=x"), None);
        assert_eq!(code_from_capture_url("http://fishy-auth.localhost.evil.com/done?code=x"), None);
        assert_eq!(code_from_capture_url("http://fishy-auth.localhost/done"), None);
    }

    #[test]
    fn parse_json_requires_a_real_code() {
        assert_eq!(parse_auth_code(r#"{"authorizationCode":"hello"}"#), None);
    }

    #[test]
    fn parse_redirect_url_with_code() {
        let url = format!("https://localhost/launcher/authorized?code={CODE}");
        assert_eq!(parse_auth_code(&url), Some(CODE.to_string()));
        let json = format!(r#"{{"redirectUrl":"https://localhost/launcher/authorized?code={CODE}","authorizationCode":null}}"#);
        assert_eq!(parse_auth_code(&json), Some(CODE.to_string()));
    }

    const REDIRECT: &str = "https://www.epicgames.com/id/api/redirect?clientId=34a02cf8f4414e29b15921876da36f9a&responseType=code";

    #[test]
    fn capture_guard_accepts_code_right_after_epic_redirect_page() {
        let mut g = CaptureGuard::default();
        assert_eq!(g.on_navigate("https://www.epicgames.com/id/login?x=1"), Navigation::Allow);
        assert_eq!(g.on_navigate(REDIRECT), Navigation::Allow);
        assert_eq!(
            g.on_navigate(&format!("http://fishy-auth.localhost/done?code={CODE}")),
            Navigation::Capture(CODE.to_string())
        );
    }

    #[test]
    fn capture_guard_blocks_codes_from_other_pages() {
        let mut g = CaptureGuard::default();
        assert_eq!(g.on_navigate("https://evil.example.com/sso"), Navigation::Allow);
        assert_eq!(g.on_navigate(&format!("http://fishy-auth.localhost/done?code={CODE}")), Navigation::Block);
    }

    #[test]
    fn capture_guard_blocks_non_codes() {
        let mut g = CaptureGuard::default();
        g.on_navigate(REDIRECT);
        assert_eq!(g.on_navigate("http://fishy-auth.localhost/done?code=hello"), Navigation::Block);
    }
}
