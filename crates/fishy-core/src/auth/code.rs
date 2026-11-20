use url::Url;

/// Where the login window's helper script sends the code; caught and cancelled in Rust.
pub const CAPTURE_PREFIX: &str = "http://fishy-auth.localhost/done";

fn is_code(s: &str) -> bool {
    s.len() == 32 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Accepts a bare authorization code, a quoted one, or Epic's whole redirect JSON.
pub fn parse_auth_code(input: &str) -> Option<String> {
    let trimmed = input.trim().trim_matches(|c| c == '"' || c == '\'').trim();
    if is_code(trimmed) {
        return Some(trimmed.to_string());
    }
    let json: serde_json::Value = serde_json::from_str(input.trim()).ok()?;
    let code = json.get("authorizationCode")?.as_str()?.trim();
    (!code.is_empty()).then(|| code.to_string())
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
}
