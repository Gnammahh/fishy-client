//! Masks secret-shaped text and the Windows user name in a log line before it's written (spec §6f.1).
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

/// The line with anything secret-shaped replaced by `[redacted]` and the user's folder by `%USERPROFILE%`.
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
    out = hide_user_names(out);
    out = mask_long_runs(out);
    out
}

/// `%USERPROFILE%` stands in for the user's own folder, so paths stay readable without their name.
const PROFILE_MASK: &str = "%USERPROFILE%";
const SEP: &str = r"(?:\\|/)+";
// `C:\Users\<name>`: a name with spaces when a folder follows it, else up to the first space.
static USERS_DIR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r#"(?i)\b[a-z]:{SEP}Users{SEP}(?:([^\\/"'<>|:*?\r\n]+?)([\\/])|([^\\/\s"'<>|:*?]+))"#)).unwrap()
});
static PROFILE: LazyLock<Option<Regex>> = LazyLock::new(|| profile_regex(&std::env::var("USERPROFILE").ok()?));

/// The profile folder spelled with any separators, ending where its name ends.
fn profile_regex(profile: &str) -> Option<Regex> {
    let parts: Vec<String> = profile.split(['\\', '/']).filter(|p| !p.is_empty()).map(regex::escape).collect();
    (parts.len() >= 2).then(|| Regex::new(&format!(r#"(?i){}([\\/\s"']|$)"#, parts.join(SEP))).ok()).flatten()
}

#[cfg(test)]
fn hide_profile<'a>(text: Cow<'a, str>, profile: Option<&str>) -> Cow<'a, str> {
    let Some(re) = profile.and_then(profile_regex) else { return text };
    hide_with(text, &re)
}

fn hide_with<'a>(text: Cow<'a, str>, re: &Regex) -> Cow<'a, str> {
    match re.replace_all(&text, |c: &Captures| format!("{PROFILE_MASK}{}", &c[1])) {
        Cow::Owned(s) => Cow::Owned(s),
        Cow::Borrowed(_) => text,
    }
}

fn hide_user_names(text: Cow<'_, str>) -> Cow<'_, str> {
    let text = match PROFILE.as_ref() {
        Some(re) => hide_with(text, re),
        None => text,
    };
    let replaced = USERS_DIR.replace_all(&text, |c: &Captures| {
        let name = c.get(1).or(c.get(3)).map_or("", |m| m.as_str());
        if ["public", "default", "all users", "default user"].contains(&name.to_ascii_lowercase().as_str()) {
            return c[0].to_string();
        }
        format!("{PROFILE_MASK}{}", c.get(2).map_or("", |m| m.as_str()))
    });
    match replaced {
        Cow::Owned(s) => Cow::Owned(s),
        Cow::Borrowed(_) => text,
    }
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
    fn hides_the_user_name_in_profile_paths() {
        assert_eq!(
            scrub(r"install queued for Bad Cheese into C:\Users\FishTester\Games\Bad Cheese"),
            r"install queued for Bad Cheese into %USERPROFILE%\Games\Bad Cheese"
        );
        assert_eq!(scrub(r"c:/users/FishTester/AppData"), r"%USERPROFILE%/AppData");
        // Debug-printed paths double their backslashes.
        assert_eq!(scrub(r#"path: "C:\\Users\\FishTester\\Games""#), r#"path: "%USERPROFILE%\\Games""#);
        // A name with a space is hidden whole when a folder follows it.
        assert_eq!(scrub(r"C:\Users\Fish Tester\Games"), r"%USERPROFILE%\Games");
        assert_eq!(scrub(r"wiped C:\Users\FishTester"), r"wiped %USERPROFILE%");
        assert_eq!(scrub(r"C:\Users\FishTester moved"), r"%USERPROFILE% moved");
        // Shared folders name nobody.
        assert_eq!(scrub(r"C:\Users\Public\Desktop"), r"C:\Users\Public\Desktop");
    }

    #[test]
    fn hides_the_real_profile_folder_wherever_it_is() {
        assert_eq!(
            hide_profile(r"wrote D:\Profiles\Fish Tester\Games\x".into(), Some(r"D:\Profiles\Fish Tester")),
            r"wrote %USERPROFILE%\Games\x"
        );
        assert_eq!(hide_profile(r"d:\profiles\fish tester\x".into(), Some(r"D:\Profiles\Fish Tester")), r"%USERPROFILE%\x");
        assert_eq!(hide_profile("nothing here".into(), Some(r"D:\Profiles\Fish Tester")), "nothing here");
        assert_eq!(hide_profile("nothing here".into(), None), "nothing here");
    }

    #[test]
    fn keeps_normal_lines_and_paths() {
        assert_eq!(
            scrub(r"C:\Users\you\Games\SomeVeryLongGameFolderNameWithoutSpaces1234\game.exe"),
            r"%USERPROFILE%\Games\SomeVeryLongGameFolderNameWithoutSpaces1234\game.exe"
        );
        for line in [
            "installed Bad Cheese (1.2 GB) in 34 s",
            r"C:\Games\SomeVeryLongGameFolderNameWithoutSpaces1234\game.exe",
            r"writing D:\Games\AnotherQuiteLongFolderNameWithNoSpacesAtAll99",
            "library refresh took 5.8 s (164 lookups)",
            "basic settings saved",
            "error_code=errors.com.epicgames.not_found",
        ] {
            assert_eq!(scrub(line), line);
        }
    }
}
