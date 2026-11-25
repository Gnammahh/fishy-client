//! Everything Fishy Client relies on from Epic, in one place: who it says it is, which servers
//! it talks to, the sign-in pages, and where Epic's own launcher keeps its files. When Epic
//! changes something, start here (docs/EPIC-CHANGES.md). A test keeps these values out of every
//! other file.

// --- Who Fishy Client says it is. The same values Legendary and Heroic use. ---

/// The Epic Games Launcher's public OAuth client.
pub const CLIENT_ID: &str = "34a02cf8f4414e29b15921876da36f9a";
pub const CLIENT_SECRET: &str = "daafbccc737745039dffe53d94fc76cf";
/// Sign-in, library, launch: Epic's account and catalog services.
pub const ACCOUNT_USER_AGENT: &str = "UELauncher/11.0.1-14907503+++Portal+Release-Live Windows/10.0.19041.1.256.64bit";
/// Game downloads (manifests and chunks from Epic's download servers).
pub const CDN_USER_AGENT: &str = "EpicGamesLauncher/11.0.1-14907503+++Portal+Release-Live Windows/10.0.19041.1.256.64bit";
/// The store's achievements service.
pub const STORE_USER_AGENT: &str = "EpicGamesLauncher/14.0.8-22004686+++Portal+Release-Live";

// --- Epic's servers. ---

pub const ACCOUNT_HOST: &str = "https://account-public-service-prod03.ol.epicgames.com";
pub const LIBRARY_HOST: &str = "https://library-service.live.use1a.on.epicgames.com";
pub const CATALOG_HOST: &str = "https://catalog-public-service-prod06.ol.epicgames.com";
pub const LAUNCHER_HOST: &str = "https://launcher-public-service-prod06.ol.epicgames.com";
pub const ECOMMERCE_HOST: &str = "https://ecommerceintegration-public-service-ecomprod02.ol.epicgames.com";
pub const STORE_GQL_HOST: &str = "https://launcher.store.epicgames.com";
pub const FREE_GAMES_URL: &str = "https://store-site-backend-static-ipv4.ak.epicgames.com/freeGamesPromotions?locale=en-US";

// --- Sign-in pages (spec §6a). ---

/// The host of Epic's login and redirect pages.
pub const LOGIN_HOST: &str = "www.epicgames.com";
/// The page that ends sign-in with JSON holding `authorizationCode`.
pub const REDIRECT_PATH: &str = "/id/api/redirect";
/// Epic's login page, sent on to [`REDIRECT_PATH`] for [`CLIENT_ID`] afterwards.
pub const LOGIN_URL: &str = "https://www.epicgames.com/id/login?redirectUrl=https%3A%2F%2Fwww.epicgames.com%2Fid%2Fapi%2Fredirect%3FclientId%3D34a02cf8f4414e29b15921876da36f9a%26responseType%3Dcode";

// --- The Store window (spec §6o). ---

/// The Store window only shows https pages on this domain or its subdomains.
pub const DOMAIN: &str = "epicgames.com";
pub const STORE_HOME: &str = "https://store.epicgames.com/";
pub const FREE_GAMES_PAGE: &str = "https://store.epicgames.com/free-games";
/// A product page is this followed by the product's slug.
pub const STORE_PRODUCT_PAGE: &str = "https://store.epicgames.com/p/";

// --- Epic's own launcher on this PC (games installed with it). ---

/// Under `%ProgramData%`: one `.item` file per game Epic's launcher installed.
pub const LAUNCHER_MANIFESTS_DIR: [&str; 4] = ["Epic", "EpicGamesLauncher", "Data", "Manifests"];
/// Inside each game's folder: Epic's file list for that game.
pub const EGSTORE_DIR: &str = ".egstore";

/// What the UI says when Epic replies with something Fishy Client doesn't understand: usually
/// Epic changed how something works.
pub const CHANGED_MESSAGE: &str = "Epic changed something. Update Fishy Client, or check back later.";

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                rust_files(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }

    #[test]
    fn epic_values_live_only_in_this_file() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut files = Vec::new();
        rust_files(&root.join("src"), &mut files);
        rust_files(&root.join("../../app/src-tauri/src"), &mut files);
        assert!(files.len() > 20, "found only {} files", files.len());
        let watched = [DOMAIN, CLIENT_ID, CLIENT_SECRET, "UELauncher/", "EpicGamesLauncher"];
        let mut found = Vec::new();
        for file in files.iter().filter(|f| !f.ends_with("epic.rs")) {
            let text = std::fs::read_to_string(file).unwrap();
            // Tests may hold sample replies; only the code that runs counts.
            let code = text.split("#[cfg(test)]").next().unwrap();
            for (i, line) in code.lines().enumerate() {
                if line.trim_start().starts_with("//") {
                    continue;
                }
                for w in watched.iter().filter(|w| line.contains(**w)) {
                    found.push(format!("{}:{}: {w}", file.display(), i + 1));
                }
            }
        }
        assert!(found.is_empty(), "Epic values outside epic.rs:\n{}", found.join("\n"));
    }

    #[test]
    fn the_login_url_asks_for_this_client_and_redirect_page() {
        let url = url::Url::parse(LOGIN_URL).unwrap();
        assert_eq!(url.host_str(), Some(LOGIN_HOST));
        let (_, redirect) = url.query_pairs().find(|(k, _)| k == "redirectUrl").unwrap();
        let redirect = url::Url::parse(&redirect).unwrap();
        assert_eq!((redirect.host_str(), redirect.path()), (Some(LOGIN_HOST), REDIRECT_PATH));
        let query: Vec<_> = redirect.query_pairs().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        assert_eq!(query, [("clientId".to_string(), CLIENT_ID.to_string()), ("responseType".into(), "code".into())]);
    }

    #[test]
    fn every_server_is_https_on_epics_domain() {
        for host in [ACCOUNT_HOST, LIBRARY_HOST, CATALOG_HOST, LAUNCHER_HOST, ECOMMERCE_HOST, STORE_GQL_HOST, FREE_GAMES_URL, STORE_HOME, FREE_GAMES_PAGE, STORE_PRODUCT_PAGE] {
            let u = url::Url::parse(host).unwrap();
            assert_eq!(u.scheme(), "https", "{host}");
            assert!(u.host_str().unwrap().ends_with(&format!(".{DOMAIN}")), "{host}");
        }
    }
}
