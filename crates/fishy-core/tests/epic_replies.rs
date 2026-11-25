//! Epic's replies, as recorded in `testdata/epic` (fake IDs and tokens), read through the real
//! clients. If Epic changes a reply, saving the new one over its file makes a test here fail
//! (docs/EPIC-CHANGES.md). The `changed_…` tests check that a reply missing what Fishy Client
//! needs is reported as "Epic changed something", never read as an empty library or a game
//! that can't be installed.
use fishy_core::auth::{AuthError, EpicClient, Secret};
use fishy_core::install::client::InstallClient;
use fishy_core::install::manifest::parse_manifest;
use fishy_core::install::InstallError;
use fishy_core::launch::LaunchClient;
use fishy_core::library::LibraryClient;
use serde_json::Value;
use sha1::{Digest, Sha1};
use std::time::Duration;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const FAST: Duration = Duration::from_millis(1);
const ITEMS: &str = "/library/api/public/items";
const ASSETS: &str = "/launcher/api/public/assets/Windows";
const INFO: &str = "/launcher/api/public/assets/v2/platform/Windows/namespace/ns-tide/catalogItem/cat-tide/app/TidepoolApp/label/Live";

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!("{}/testdata/epic/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn json(name: &str) -> Value {
    serde_json::from_str(&fixture(name)).unwrap()
}

/// `v` with `key` renamed wherever it appears, as if Epic had renamed that field.
fn renamed(mut v: Value, key: &str) -> Value {
    match &mut v {
        Value::Object(m) => {
            if let Some(x) = m.remove(key) {
                m.insert(format!("{key}Renamed"), x);
            }
            for x in m.values_mut() {
                *x = renamed(x.take(), key);
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| *x = renamed(x.take(), key)),
        _ => {}
    }
    v
}

async fn serve(server: &MockServer, p: &str, body: Value) {
    Mock::given(path(p)).respond_with(ResponseTemplate::new(200).set_body_json(body)).mount(server).await;
}

fn token() -> Secret {
    Secret::new("eg1~FIXTURE")
}

// --- Sign-in ---

#[tokio::test]
async fn sign_in_reads_epics_token_reply() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/account/api/oauth/token")).respond_with(ResponseTemplate::new(200).set_body_string(fixture("oauth-token.json"))).mount(&server).await;
    let s = EpicClient::with_base_url(&server.uri()).exchange_code("code").await.unwrap();
    assert_eq!(s.access_token.expose(), "eg1~FIXTURE-ACCESS-TOKEN");
    assert_eq!(s.refresh_token.expose(), "eg1~FIXTURE-REFRESH-TOKEN");
    assert_eq!(s.account_id, "f15h0000000000000000000000000001");
    assert_eq!(s.display_name, "FixtureFish");
    assert_eq!(s.expires_at.to_rfc3339(), "2026-11-25T18:00:00+00:00");
    assert_eq!(s.refresh_expires_at.to_rfc3339(), "2026-12-18T16:00:00+00:00");
}

#[tokio::test]
async fn changed_token_reply_is_reported_as_epic_changed() {
    for key in ["access_token", "refresh_token", "expires_at", "account_id", "displayName"] {
        let server = MockServer::start().await;
        serve(&server, "/account/api/oauth/token", renamed(json("oauth-token.json"), key)).await;
        let e = EpicClient::with_base_url(&server.uri()).exchange_code("code").await.unwrap_err();
        assert!(matches!(e, AuthError::Unexpected(_)), "{key}: {e:?}");
    }
}

// --- Library ---

async fn library_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(path(ITEMS)).and(query_param("cursor", "cursor-page-2")).respond_with(ResponseTemplate::new(200).set_body_json(json("library-items-page2.json"))).mount(&server).await;
    serve(&server, ITEMS, json("library-items-page1.json")).await;
    serve(&server, ASSETS, json("assets-windows.json")).await;
    serve(&server, "/catalog/api/shared/namespace/ns-tide/bulk/items", json("catalog-bulk.json")).await;
    serve(&server, "/catalog/api/shared/namespace/ns-reef/bulk/items", serde_json::json!({})).await;
    server
}

fn library(server: &MockServer) -> LibraryClient {
    LibraryClient::with_base_urls(&server.uri(), &server.uri(), &server.uri()).with_retry_base(FAST)
}

#[tokio::test]
async fn library_reads_epics_items_downloads_and_game_details() {
    let server = library_server().await;
    let data = library(&server).fetch_library(&token()).await.unwrap();
    let mut games = data.games;
    games.sort_by(|a, b| a.app_name.cmp(&b.app_name));
    assert_eq!(games.iter().map(|g| g.app_name.as_str()).collect::<Vec<_>>(), ["ReefApp", "TidepoolApp"], "both pages read");
    let tide = &games[1];
    assert_eq!((tide.namespace.as_str(), tide.catalog_item_id.as_str()), ("ns-tide", "cat-tide"));
    assert_eq!(tide.title, "Tidepool Odyssey");
    assert_eq!(tide.cover_url.as_deref(), Some("https://cdn1.epicgames.com/ns-tide/tall.jpg"));
    assert!(tide.installable);
    assert!(tide.launch.can_run_offline);
    assert_eq!(tide.launch.additional_command_line.as_deref(), Some("-nosplash"));
    assert_eq!(data.latest_versions.get("TidepoolApp").map(String::as_str), Some("1.4.2-CL-2026"));
    assert_eq!(data.latest_versions.get("ReefApp").map(String::as_str), Some("++Reef+Release-0.9-CL-77"));
}

#[tokio::test]
async fn changed_library_items_are_reported_not_shown_as_an_empty_library() {
    for key in ["records", "catalogItemId", "namespace"] {
        let server = MockServer::start().await;
        serve(&server, ITEMS, renamed(json("library-items-page2.json"), key)).await;
        let e = library(&server).items(&token()).await.unwrap_err();
        assert!(matches!(e, AuthError::Unexpected(_)), "{key}: {e:?}");
    }
}

#[tokio::test]
async fn changed_windows_downloads_list_is_reported_not_read_as_nothing_installable() {
    let server = MockServer::start().await;
    serve(&server, ASSETS, renamed(json("assets-windows.json"), "appName")).await;
    let e = library(&server).windows_assets(&token()).await.unwrap_err();
    assert!(matches!(e, AuthError::Unexpected(_)), "{e:?}");

    // An empty list is a real answer (no Windows games), not a change.
    let server = MockServer::start().await;
    serve(&server, ASSETS, serde_json::json!([])).await;
    assert!(library(&server).windows_assets(&token()).await.unwrap().is_empty());
}

#[tokio::test]
async fn changed_game_details_are_reported_not_shown_as_untitled_games() {
    let server = MockServer::start().await;
    serve(&server, "/catalog/api/shared/namespace/ns-tide/bulk/items", renamed(json("catalog-bulk.json"), "title")).await;
    let e = library(&server).catalog_item(&token(), "ns-tide", "cat-tide").await.unwrap_err();
    assert!(matches!(e, AuthError::Unexpected(_)), "{e:?}");

    // When every game's details are unreadable, the whole refresh says so.
    let server = MockServer::start().await;
    Mock::given(path(ITEMS)).and(query_param("cursor", "cursor-page-2")).respond_with(ResponseTemplate::new(200).set_body_json(json("library-items-page2.json"))).mount(&server).await;
    serve(&server, ITEMS, json("library-items-page1.json")).await;
    serve(&server, ASSETS, json("assets-windows.json")).await;
    let reef = serde_json::json!({ "cat-reef": json("catalog-bulk.json")["cat-tide"].clone() });
    serve(&server, "/catalog/api/shared/namespace/ns-tide/bulk/items", renamed(json("catalog-bulk.json"), "title")).await;
    serve(&server, "/catalog/api/shared/namespace/ns-reef/bulk/items", renamed(reef, "title")).await;
    let e = library(&server).fetch_library(&token()).await.unwrap_err();
    assert!(matches!(e, AuthError::Unexpected(_)), "{e:?}");
}

// --- Downloads ---

#[tokio::test]
async fn downloads_read_epics_download_info_and_file_list() {
    let server = MockServer::start().await;
    let manifest = fixture("manifest.json");
    let hash = hex::encode(Sha1::digest(manifest.as_bytes()));
    let info = fixture("manifest-info.json").replace("{{SERVER}}", &server.uri()).replace("{{HASH}}", &hash.to_uppercase());
    serve(&server, INFO, serde_json::from_str(&info).unwrap()).await;
    Mock::given(path("/Builds/Org/o-fixture/tidepool/default/TidepoolApp.manifest")).respond_with(ResponseTemplate::new(200).set_body_string(manifest.clone())).mount(&server).await;

    let client = InstallClient::with_base_url(&server.uri()).with_retry_base(FAST);
    let info = client.manifest_info(&token(), "ns-tide", "cat-tide", "TidepoolApp").await.unwrap();
    assert_eq!(info.hash, hash, "lower-cased");
    assert_eq!(info.build_version, "1.4.2-CL-2026");
    let base = format!("{}/Builds/Org/o-fixture/tidepool/default", server.uri());
    assert_eq!(info.base_urls, std::slice::from_ref(&base));
    assert_eq!(info.urls, [format!("{base}/TidepoolApp.manifest?f_token=exp=1790000000~acl=%2F*~hmac=fixture"), format!("{base}/TidepoolApp.manifest")]);

    let bytes = client.download_manifest(&info).await.unwrap();
    let m = parse_manifest(&bytes).unwrap();
    assert_eq!((m.meta.app_name.as_str(), m.meta.build_version.as_str()), ("TidepoolApp", "1.4.2-CL-2026"));
    assert_eq!(m.meta.launch_exe, "Binaries/Win64/Tidepool.exe");
    assert_eq!(m.chunk_dir(), "ChunksV4");
    assert_eq!(m.files.iter().map(|f| (f.filename.as_str(), f.size())).collect::<Vec<_>>(), [("Binaries/Win64/Tidepool.exe", 1000), ("Content/Paks/tide.pak", 24)]);
    assert!(m.files[0].executable);
    let c = m.chunk(&[1, 2, 3, 10]).unwrap();
    assert_eq!((c.hash, c.group, c.file_size), (0x1A2B3C, 7, 900));
}

#[tokio::test]
async fn changed_download_info_is_reported_as_epic_changed() {
    for key in ["elements", "manifests", "uri"] {
        let server = MockServer::start().await;
        let info = fixture("manifest-info.json").replace("{{SERVER}}", &server.uri());
        serve(&server, INFO, renamed(serde_json::from_str(&info).unwrap(), key)).await;
        let e = InstallClient::with_base_url(&server.uri()).with_retry_base(FAST).manifest_info(&token(), "ns-tide", "cat-tide", "TidepoolApp").await.unwrap_err();
        assert!(matches!(e, InstallError::EpicChanged(_)), "{key}: {e:?}");
    }
}

#[tokio::test]
async fn changed_file_list_is_refused() {
    for key in ["FileManifestList", "ChunkHashList", "Filename"] {
        let bytes = renamed(json("manifest.json"), key).to_string();
        assert!(matches!(parse_manifest(bytes.as_bytes()), Err(InstallError::BadManifest(_))), "{key}");
    }
}

// --- Play ---

#[tokio::test]
async fn play_reads_epics_one_time_code() {
    let server = MockServer::start().await;
    serve(&server, "/account/api/oauth/exchange", json("exchange-code.json")).await;
    let code = LaunchClient::with_base_urls(&server.uri(), &server.uri()).with_retry_base(FAST).exchange_code(&token()).await.unwrap();
    assert_eq!(code.expose(), "fixture0exchange0code0000000001");

    let server = MockServer::start().await;
    serve(&server, "/account/api/oauth/exchange", renamed(json("exchange-code.json"), "code")).await;
    let e = LaunchClient::with_base_urls(&server.uri(), &server.uri()).with_retry_base(FAST).exchange_code(&token()).await.unwrap_err();
    assert!(matches!(e, AuthError::Unexpected(_)), "{e:?}");
}
