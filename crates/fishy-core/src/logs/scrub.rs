//! Masks secret-shaped text in a log line before it's written (spec §6f.1).
use regex::{Captures, Regex};
use std::borrow::Cow;
use std::sync::LazyLock;

const MASK: &str = "[redacted]";
const KEYS: &str = "code|token|access_?token|refresh_?token|exchange_?code|password";

static AUTH_HEADER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(authorization:\s*)(?:(?:bearer|basic)\s+)?\S+").unwrap());
static BEARER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\b(bearer\s+)\S+").unwrap());
// "basic" is an ordinary word too, so only a base64-looking value after it counts.
static BASIC: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\b(basic\s+)[A-Za-z0-9+/]{8,}={0,2}").unwrap());
static KEY_EQ: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r#"(?i)\b((?:{KEYS})=)[^&\s"',;]+"#)).unwrap());
static KEY_JSON: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r#"(?i)("(?:{KEYS})"\s*:\s*")[^"]*""#)).unwrap());
static EG1: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"eg1~[A-Za-z0-9._\-+/=]+").unwrap());
static LONG_RUN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[A-Za-z0-9\-_+/=]{32,}").unwrap());

/// The line with anything secret-shaped replaced by `[redacted]`.
pub fn scrub(line: &str) -> Cow<'_, str> {
    let mut out = Cow::Borrowed(line);
    let mut apply = |re: &Regex, f: &dyn Fn(&Captures) -> String| {
        if let Cow::Owned(s) = re.replace_all(&out, |c: &Captures| f(c)) {
            out = Cow::Owned(s);
        }
    };
    apply(&AUTH_HEADER, &|c| format!("{}{MASK}", &c[1]));
    apply(&EG1, &|_| MASK.to_string());
    apply(&BEARER, &|c| if &c[0][c[1].len()..] == MASK { c[0].to_string() } else { format!("{}{MASK}", &c[1]) });
    apply(&BASIC, &|c| {
        let value = &c[0][c[1].len()..];
        let encoded = value.len() >= 16 || value.contains(|ch: char| ch.is_ascii_digit() || "+/=".contains(ch));
        if encoded { format!("{}{MASK}", &c[1]) } else { c[0].to_string() }
    });
    apply(&KEY_EQ, &|c| format!("{}{MASK}", &c[1]));
    apply(&KEY_JSON, &|c| format!("{}{MASK}\"", &c[1]));
    out = mask_long_runs(out);
    out
}

/// A drive like `C:\` (a letter before `:\`), not JSON's `":\"`.
fn has_drive(s: &str) -> bool {
    let b = s.as_bytes();
    s.match_indices(":\\").any(|(i, _)| i >= 1 && b[i - 1].is_ascii_alphabetic())
}

/// Long random strings, except where they are a part of a Windows path.
fn mask_long_runs(text: Cow<'_, str>) -> Cow<'_, str> {
    let bytes = text.as_bytes();
    let hits: Vec<(usize, usize)> = LONG_RUN
        .find_iter(&text)
        .filter(|m| {
            let before = m.start().checked_sub(1).map(|i| bytes[i]);
            let after = bytes.get(m.end()).copied();
            let word_start = text[..m.start()].rfind(char::is_whitespace).map_or(0, |i| i + 1);
            // `\"` after a value is an escaped quote in JSON text, not a path.
            let escaped_quote = after == Some(b'\\') && bytes.get(m.end() + 1) == Some(&b'"');
            let in_path = before == Some(b'\\') || (after == Some(b'\\') && !escaped_quote) || has_drive(&text[word_start..m.start()]);
            !in_path
        })
        .map(|m| (m.start(), m.end()))
        .collect();
    if hits.is_empty() {
        return text;
    }
    let mut s = String::with_capacity(text.len());
    let mut last = 0;
    for (a, b) in hits {
        s.push_str(&text[last..a]);
        s.push_str(MASK);
        last = b;
    }
    s.push_str(&text[last..]);
    Cow::Owned(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_authorization_values() {
        assert_eq!(scrub("Authorization: bearer abc.def"), "Authorization: [redacted]");
        assert_eq!(scrub("authorization: basic dGVzdDp0ZXN0"), "authorization: [redacted]");
        assert_eq!(scrub("sent bearer eg1~xyz"), "sent bearer [redacted]");
        assert_eq!(scrub("Bearer abcdefgh123"), "Bearer [redacted]");
    }

    #[test]
    fn masks_key_values() {
        assert_eq!(scrub("code=ABC123&x=1"), "code=[redacted]&x=1");
        assert_eq!(scrub(r#"{"refresh_token": "r1"}"#), r#"{"refresh_token": "[redacted]"}"#);
        for key in ["code", "token", "access_token", "refresh_token", "exchange_code", "password"] {
            assert_eq!(scrub(&format!("x {key}=s3cret y")), format!("x {key}=[redacted] y"), "{key}");
            assert_eq!(scrub(&format!(r#"{{"{key}":"s3cret"}}"#)), format!(r#"{{"{key}":"[redacted]"}}"#), "{key}");
        }
        assert_eq!(scrub("PASSWORD=hunter2"), "PASSWORD=[redacted]");
    }

    #[test]
    fn masks_camel_case_keys_and_values_before_an_escaped_quote() {
        assert_eq!(scrub(r#"{"accessToken":"s3cret"}"#), r#"{"accessToken":"[redacted]"}"#);
        assert_eq!(scrub("refreshToken=s3cret x"), "refreshToken=[redacted] x");
        assert_eq!(scrub("exchangeCode=s3cret"), "exchangeCode=[redacted]");
        let forty = "0123456789abcdef0123456789abcdef01234567";
        assert_eq!(scrub(&format!(r#"body {{\"id\":\"{forty}\"}}"#)), r#"body {\"id\":\"[redacted]\"}"#);
    }

    #[test]
    fn masks_eg1_tokens() {
        assert_eq!(scrub("got eg1~QWERTY.zxc-1"), "got [redacted]");
    }

    #[test]
    fn masks_long_random_runs() {
        let forty = "0123456789abcdef0123456789abcdef01234567";
        assert_eq!(scrub(&format!("id {forty} done")), "id [redacted] done");
        let thirty_one = &forty[..31];
        assert_eq!(scrub(&format!("id {thirty_one} done")), format!("id {thirty_one} done"));
    }

    #[test]
    fn keeps_normal_lines_and_paths() {
        for line in [
            "installed Bad Cheese (1.2 GB) in 34 s",
            r"C:\Users\you\Games\SomeVeryLongGameFolderNameWithoutSpaces1234\game.exe",
            r"writing D:\Games\AnotherQuiteLongFolderNameWithNoSpacesAtAll99",
            "library refresh took 5.8 s (164 lookups)",
            "basic settings saved",
            "error_code=errors.com.epicgames.not_found",
        ] {
            assert_eq!(scrub(line), line);
        }
    }
}
