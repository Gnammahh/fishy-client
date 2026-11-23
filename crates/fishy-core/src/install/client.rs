//! Talks to Epic for installs: manifest info (signed in), then manifest and chunks from the CDN (no auth).
use super::chunk::{chunk_path, decode_chunk};
use super::manifest::ChunkInfo;
use super::InstallError;
use crate::auth::client::{http_client, network, read_json};
use crate::auth::{AuthError, Secret};
use reqwest::Client;
use serde_json::Value;
use sha1::{Digest, Sha1};
use std::time::Duration;

pub const LAUNCHER_HOST: &str = "https://launcher-public-service-prod06.ol.epicgames.com";
const CDN_USER_AGENT: &str = "EpicGamesLauncher/11.0.1-14907503+++Portal+Release-Live Windows/10.0.19041.1.256.64bit";
/// Like Legendary's 10 s: how long the CDN may stay silent, not how long a whole download may take.
const CDN_TIMEOUT: Duration = Duration::from_secs(10);
const CHUNK_TRIES: u32 = 7;

/// Where a build's manifest is and which CDNs serve its chunks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestInfo {
    /// SHA-1 (hex) of the manifest file.
    pub hash: String,
    pub urls: Vec<String>,
    pub base_urls: Vec<String>,
    pub build_version: String,
}

/// Connect and read timeouts only: a slow line that keeps sending data is never cut off.
fn cdn_client(silence: Duration) -> Client {
    Client::builder().user_agent(CDN_USER_AGENT).connect_timeout(silence).read_timeout(silence).build().expect("http client")
}

#[derive(Clone)]
pub struct InstallClient {
    api: Client,
    cdn: Client,
    launcher_base: String,
    retry_base: Duration,
    /// Wait before retrying an Epic API call or the file list (spec §6f.2).
    api_retry_base: Duration,
}

impl Default for InstallClient {
    fn default() -> Self {
        Self::new()
    }
}

impl InstallClient {
    pub fn new() -> Self {
        Self::with_base_url(LAUNCHER_HOST)
    }

    /// For tests: a fake launcher service.
    pub fn with_base_url(launcher: &str) -> Self {
        Self { api: http_client(), cdn: cdn_client(CDN_TIMEOUT), launcher_base: launcher.trim_end_matches('/').to_string(), retry_base: Duration::from_secs(1), api_retry_base: crate::retry::RETRY_BASE }
    }

    /// For tests: shorter waits between retries (chunks and API calls).
    pub fn with_retry_base(mut self, d: Duration) -> Self {
        self.retry_base = d;
        self.api_retry_base = d;
        self
    }

    /// For tests: a shorter CDN timeout.
    pub fn with_cdn_timeout(mut self, d: Duration) -> Self {
        self.cdn = cdn_client(d);
        self
    }

    pub async fn manifest_info(&self, token: &Secret, namespace: &str, catalog_item_id: &str, app_name: &str) -> Result<ManifestInfo, InstallError> {
        crate::retry::with_retries("download info", self.api_retry_base, || self.manifest_info_once(token, namespace, catalog_item_id, app_name)).await
    }

    async fn manifest_info_once(&self, token: &Secret, namespace: &str, catalog_item_id: &str, app_name: &str) -> Result<ManifestInfo, InstallError> {
        let url = format!(
            "{}/launcher/api/public/assets/v2/platform/Windows/namespace/{namespace}/catalogItem/{catalog_item_id}/app/{app_name}/label/Live",
            self.launcher_base
        );
        let res = self.api.get(url).header("Authorization", format!("bearer {}", token.expose())).send().await.map_err(network)?;
        let status = res.status().as_u16();
        let json = read_json(res).await.map_err(|e| match e {
            AuthError::Rejected { code } => match status {
                // A server hiccup isn't a sign-in problem.
                500.. => InstallError::Network(code),
                _ if code.ends_with("download_info_not_found") => InstallError::NoDownload,
                401 => InstallError::Rejected(code),
                _ if code.contains("token") => InstallError::Rejected(code),
                // Not owned, no Windows build, ...: signing in again won't help.
                _ => InstallError::BadManifest(code),
            },
            other => other.into(),
        })?;
        let elements = json.get("elements").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
        let [element] = elements else {
            return Err(InstallError::BadManifest(format!("{} elements", elements.len())));
        };
        let text = |k: &str| element.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
        let mut urls = Vec::new();
        let mut base_urls: Vec<String> = Vec::new();
        for m in element.get("manifests").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default() {
            let Some(uri) = m.get("uri").and_then(Value::as_str) else { continue };
            let base = uri.rsplit_once('/').map_or(uri, |(b, _)| b).to_string();
            if !base_urls.contains(&base) {
                base_urls.push(base);
            }
            let params: Vec<String> = m
                .get("queryParams")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .filter_map(|p| Some(format!("{}={}", p.get("name")?.as_str()?, p.get("value")?.as_str()?)))
                .collect();
            urls.push(if params.is_empty() { uri.to_string() } else { format!("{uri}?{}", params.join("&")) });
        }
        if urls.is_empty() {
            return Err(InstallError::BadManifest("no manifest urls".into()));
        }
        Ok(ManifestInfo { hash: text("hash").to_lowercase(), urls, base_urls, build_version: text("buildVersion") })
    }

    /// Tries each manifest URL in order; the bytes must match `info.hash`.
    pub async fn download_manifest(&self, info: &ManifestInfo) -> Result<Vec<u8>, InstallError> {
        crate::retry::with_retries("file list", self.api_retry_base, || self.download_manifest_once(info)).await
    }

    /// Tries each manifest URL once, in order.
    async fn download_manifest_once(&self, info: &ManifestInfo) -> Result<Vec<u8>, InstallError> {
        let mut last = InstallError::Network("no manifest url".into());
        for url in &info.urls {
            match self.cdn.get(url).send().await {
                Ok(res) if res.status().is_success() => match res.bytes().await {
                    Ok(bytes) => {
                        // Not the real file (an HTML error page, a damaged copy): try the next server.
                        if hex::encode(Sha1::digest(&bytes)) == info.hash {
                            return Ok(bytes.to_vec());
                        }
                        last = InstallError::Network("manifest hash mismatch".into());
                    }
                    Err(e) => last = InstallError::Network(e.without_url().to_string()),
                },
                Ok(res) => last = InstallError::Network(format!("http_{}", res.status().as_u16())),
                Err(e) => last = InstallError::Network(e.without_url().to_string()),
            }
        }
        Err(last)
    }

    /// One chunk, decoded and verified. Retried up to 7 times, waiting 1, 2, 4… × retry_base.
    /// Each try goes to the next server in `bases` (spec §6m.2), wrapping round.
    pub async fn download_chunk(&self, bases: &[String], dir: &str, chunk: &ChunkInfo) -> Result<Vec<u8>, InstallError> {
        if bases.is_empty() {
            return Err(InstallError::BadManifest("no CDN".into()));
        }
        let mut last = InstallError::Network("not tried".into());
        for attempt in 0..CHUNK_TRIES {
            if attempt > 0 {
                tokio::time::sleep(self.retry_base * 2u32.pow(attempt - 1)).await;
            }
            match self.fetch_chunk_once(&bases[attempt as usize % bases.len()], dir, chunk).await {
                Ok(data) => return Ok(data),
                Err(InstallError::Unsupported) => return Err(InstallError::Unsupported),
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    /// One try at one chunk from one server.
    async fn fetch_chunk_once(&self, base_url: &str, dir: &str, chunk: &ChunkInfo) -> Result<Vec<u8>, InstallError> {
        let url = format!("{}/{}", base_url.trim_end_matches('/'), chunk_path(dir, chunk));
        match self.cdn.get(&url).send().await {
            Ok(res) if res.status().is_success() => match res.bytes().await {
                Ok(bytes) => decode_chunk(&bytes, chunk),
                Err(e) => Err(InstallError::Network(e.without_url().to_string())),
            },
            Ok(res) => Err(InstallError::Network(format!("http_{}", res.status().as_u16()))),
            Err(e) => Err(InstallError::Network(e.without_url().to_string())),
        }
    }

    /// The servers, fastest first (spec §6m.2): each fetches `sample` once, all at the same time.
    /// Servers that fail go last, in their old order. One server (or none) is returned as is.
    pub async fn fastest_first(&self, bases: &[String], dir: &str, sample: &[ChunkInfo]) -> Vec<String> {
        if bases.len() < 2 || sample.is_empty() {
            return bases.to_vec();
        }
        let timed = futures::future::join_all(bases.iter().map(|base| async move {
            let start = std::time::Instant::now();
            for chunk in sample {
                self.fetch_chunk_once(base, dir, chunk).await.ok()?;
            }
            Some(start.elapsed())
        }))
        .await;
        let mut order: Vec<(usize, Option<std::time::Duration>)> = timed.into_iter().enumerate().collect();
        // None sorts after every time; a stable sort keeps failed servers in their old order.
        order.sort_by_key(|&(_, t)| (t.is_none(), t));
        order.into_iter().map(|(i, _)| bases[i].clone()).collect()
    }
}

/// Just the server's name from a download address, for the log (never the path).
pub fn host_of(url: &str) -> String {
    url.split_once("://")
        .map(|(_, rest)| rest.split('/').next().unwrap_or_default())
        .filter(|h| !h.is_empty())
        .unwrap_or("?")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::chunk::{build_chunk, info_for};
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const INFO_PATH: &str = "/launcher/api/public/assets/v2/platform/Windows/namespace/ns/catalogItem/cat/app/Tide/label/Live";

    fn sha_hex(b: &[u8]) -> String {
        hex::encode(Sha1::digest(b))
    }

    /// A raw HTTP server that sends `body` in `pieces`, `gap` apart: slow but never silent for long.
    async fn trickle_server(body: Vec<u8>, pieces: usize, gap: Duration) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut s, _)) = listener.accept().await {
                let body = body.clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 4096];
                    let _ = s.read(&mut buf).await;
                    let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                    let _ = s.write_all(head.as_bytes()).await;
                    for piece in body.chunks(body.len().div_ceil(pieces)) {
                        let _ = s.write_all(piece).await;
                        let _ = s.flush().await;
                        tokio::time::sleep(gap).await;
                    }
                });
            }
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn a_slow_but_steady_chunk_is_not_cut_off() {
        let data = b"slow fish ".repeat(200);
        let info = info_for([1, 2, 3, 4], &data, 10);
        // 5 pieces 250 ms apart: about 1.25 s in total, never silent for 400 ms.
        let base = trickle_server(build_chunk(info.guid, &data, false, None), 5, Duration::from_millis(250)).await;
        let c = InstallClient::with_base_url(&base).with_retry_base(Duration::from_millis(1)).with_cdn_timeout(Duration::from_millis(400));
        assert_eq!(c.download_chunk(std::slice::from_ref(&base), "ChunksV4", &info).await.unwrap(), data);
    }

    #[tokio::test]
    async fn garbage_manifest_tries_the_next_url() {
        let server = MockServer::start().await;
        let body = b"real manifest".to_vec();
        Mock::given(method("GET")).and(path("/a.manifest")).respond_with(ResponseTemplate::new(200).set_body_string("<html>portal</html>")).mount(&server).await;
        Mock::given(method("GET")).and(path("/b.manifest")).respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone())).mount(&server).await;
        let c = InstallClient::with_base_url(&server.uri()).with_retry_base(std::time::Duration::from_millis(1));
        let info = ManifestInfo {
            hash: sha_hex(&body),
            urls: vec![format!("{}/a.manifest", server.uri()), format!("{}/b.manifest", server.uri())],
            base_urls: vec![],
            build_version: "1".into(),
        };
        assert_eq!(c.download_manifest(&info).await.unwrap(), body);
        let only_bad = ManifestInfo { urls: vec![format!("{}/a.manifest", server.uri())], ..info };
        assert!(matches!(c.download_manifest(&only_bad).await, Err(InstallError::Network(_))));
    }

    #[tokio::test]
    async fn epic_errors_are_mapped_by_meaning() {
        async fn status_of(res: ResponseTemplate) -> Result<ManifestInfo, InstallError> {
            let server = MockServer::start().await;
            Mock::given(method("GET")).and(path(INFO_PATH)).respond_with(res).mount(&server).await;
            InstallClient::with_base_url(&server.uri()).with_retry_base(std::time::Duration::from_millis(1)).manifest_info(&Secret::new("x"), "ns", "cat", "Tide").await
        }
        assert!(matches!(status_of(ResponseTemplate::new(401)).await, Err(InstallError::Rejected(_))));
        let invalid_token = serde_json::json!({"errorCode": "errors.com.epicgames.common.oauth.invalid_token"});
        assert!(matches!(status_of(ResponseTemplate::new(400).set_body_json(invalid_token)).await, Err(InstallError::Rejected(_))));
        // Not owned / no Windows build: retrying or signing in again won't help.
        assert!(matches!(status_of(ResponseTemplate::new(403)).await, Err(InstallError::BadManifest(_))));
        assert!(matches!(status_of(ResponseTemplate::new(404)).await, Err(InstallError::BadManifest(_))));
        // Seen for real (Bendy and the Ink Machine, 2026-11-21): a library entry with no Windows build.
        let none = serde_json::json!({"errorCode": "errors.com.epicgames.launcher.download_info_not_found"});
        assert_eq!(status_of(ResponseTemplate::new(404).set_body_json(none)).await, Err(InstallError::NoDownload));
        let busy = serde_json::json!({"errorCode": "errors.com.epicgames.common.server_error"});
        assert!(matches!(status_of(ResponseTemplate::new(503).set_body_json(busy)).await, Err(InstallError::Network(_))));
    }

    #[tokio::test]
    async fn manifest_info_builds_urls_and_base_urls() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(INFO_PATH))
            .and(header("authorization", "bearer eg1~A"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "elements": [{
                    "appName": "Tide", "buildVersion": "1.0", "hash": "abc",
                    "manifests": [
                        { "uri": "https://cdn1.example/Builds/x/y.manifest", "queryParams": [{"name": "a", "value": "1"}, {"name": "b", "value": "2"}] },
                        { "uri": "https://cdn1.example/Builds/x/z.manifest" },
                        { "uri": "https://cdn2.example/Builds/x/y.manifest" }
                    ]
                }]
            })))
            .expect(1)
            .mount(&server)
            .await;
        let c = InstallClient::with_base_url(&server.uri()).with_retry_base(std::time::Duration::from_millis(1));
        let info = c.manifest_info(&Secret::new("eg1~A"), "ns", "cat", "Tide").await.unwrap();
        assert_eq!(info.hash, "abc");
        assert_eq!(info.build_version, "1.0");
        assert_eq!(info.urls, vec![
            "https://cdn1.example/Builds/x/y.manifest?a=1&b=2",
            "https://cdn1.example/Builds/x/z.manifest",
            "https://cdn2.example/Builds/x/y.manifest",
        ]);
        assert_eq!(info.base_urls, vec!["https://cdn1.example/Builds/x", "https://cdn2.example/Builds/x"]);
    }

    #[tokio::test]
    async fn manifest_info_retries_after_server_errors() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path(INFO_PATH)).respond_with(ResponseTemplate::new(503)).up_to_n_times(2).mount(&server).await;
        Mock::given(method("GET"))
            .and(path(INFO_PATH))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "elements": [{"appName": "Tide", "buildVersion": "1.0", "hash": "abc", "manifests": [{"uri": "https://cdn1.example/x/y.manifest"}]}]
            })))
            .expect(1)
            .mount(&server)
            .await;
        let c = InstallClient::with_base_url(&server.uri()).with_retry_base(std::time::Duration::from_millis(1));
        assert_eq!(c.manifest_info(&Secret::new("eg1~A"), "ns", "cat", "Tide").await.unwrap().build_version, "1.0");
    }

    #[tokio::test]
    async fn manifest_info_404_is_asked_once() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path(INFO_PATH)).respond_with(ResponseTemplate::new(404)).expect(1).mount(&server).await;
        let c = InstallClient::with_base_url(&server.uri()).with_retry_base(std::time::Duration::from_millis(1));
        assert!(c.manifest_info(&Secret::new("eg1~A"), "ns", "cat", "Tide").await.is_err());
    }

    #[tokio::test]
    async fn download_manifest_retries_when_every_server_fails_then_recovers() {
        let server = MockServer::start().await;
        let body = b"manifest bytes".to_vec();
        Mock::given(method("GET")).and(path("/only.manifest")).respond_with(ResponseTemplate::new(503)).up_to_n_times(2).mount(&server).await;
        Mock::given(method("GET")).and(path("/only.manifest")).respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone())).mount(&server).await;
        let c = InstallClient::with_base_url(&server.uri()).with_retry_base(std::time::Duration::from_millis(1));
        let info = ManifestInfo { hash: sha_hex(&body), urls: vec![format!("{}/only.manifest", server.uri())], base_urls: vec![], build_version: "1".into() };
        assert_eq!(c.download_manifest(&info).await.unwrap(), body);
    }

    #[tokio::test]
    async fn manifest_info_errors() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path(INFO_PATH)).respond_with(ResponseTemplate::new(401)).mount(&server).await;
        let c = InstallClient::with_base_url(&server.uri()).with_retry_base(std::time::Duration::from_millis(1));
        assert!(matches!(c.manifest_info(&Secret::new("x"), "ns", "cat", "Tide").await, Err(InstallError::Rejected(_))));

        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path(INFO_PATH)).respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"elements": []}))).mount(&server).await;
        let c = InstallClient::with_base_url(&server.uri()).with_retry_base(std::time::Duration::from_millis(1));
        assert!(matches!(c.manifest_info(&Secret::new("x"), "ns", "cat", "Tide").await, Err(InstallError::BadManifest(_))));

        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path(INFO_PATH)).respond_with(ResponseTemplate::new(503)).mount(&server).await;
        let c = InstallClient::with_base_url(&server.uri()).with_retry_base(std::time::Duration::from_millis(1));
        assert!(matches!(c.manifest_info(&Secret::new("x"), "ns", "cat", "Tide").await, Err(InstallError::Network(_))));
    }

    #[tokio::test]
    async fn download_manifest_tries_next_url_checks_hash_and_sends_no_token() {
        let server = MockServer::start().await;
        let body = b"manifest bytes".to_vec();
        Mock::given(method("GET")).and(path("/a.manifest")).respond_with(ResponseTemplate::new(500)).mount(&server).await;
        Mock::given(method("GET")).and(path("/b.manifest")).and(query_param("k", "v")).respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone())).mount(&server).await;
        let c = InstallClient::with_base_url(&server.uri()).with_retry_base(std::time::Duration::from_millis(1));
        let info = ManifestInfo {
            hash: sha_hex(&body),
            urls: vec![format!("{}/a.manifest", server.uri()), format!("{}/b.manifest?k=v", server.uri())],
            base_urls: vec![],
            build_version: "1".into(),
        };
        assert_eq!(c.download_manifest(&info).await.unwrap(), body);
        for req in server.received_requests().await.unwrap() {
            assert!(req.headers.get("authorization").is_none(), "manifest requests must not carry the token");
        }
        let wrong = ManifestInfo { hash: "00".into(), ..info.clone() };
        assert!(matches!(c.download_manifest(&wrong).await, Err(InstallError::Network(_))));
        let none = ManifestInfo { urls: vec![format!("{}/a.manifest", server.uri())], ..info };
        assert!(matches!(c.download_manifest(&none).await, Err(InstallError::Network(_))));
    }

    #[tokio::test]
    async fn bad_chunk_is_retried_then_errors() {
        let server = MockServer::start().await;
        let info = info_for([1, 2, 3, 4], b"data", 10);
        Mock::given(method("GET"))
            .and(path(format!("/base/{}", chunk_path("ChunksV4", &info))))
            .respond_with(ResponseTemplate::new(200).set_body_string("<html>oops</html>"))
            .expect(7)
            .mount(&server)
            .await;
        let c = InstallClient::with_base_url(&server.uri()).with_retry_base(Duration::from_millis(1));
        let r = c.download_chunk(&[format!("{}/base", server.uri())], "ChunksV4", &info).await;
        assert!(matches!(r, Err(InstallError::BadChunk(_))), "{r:?}");
    }

    #[tokio::test]
    async fn chunk_recovers_after_one_failure_and_sends_no_token() {
        let server = MockServer::start().await;
        let data = b"fishy data".repeat(10);
        let info = info_for([1, 2, 3, 4], &data, 10);
        let p = format!("/base/{}", chunk_path("ChunksV4", &info));
        Mock::given(method("GET")).and(path(p.clone())).respond_with(ResponseTemplate::new(503)).up_to_n_times(1).mount(&server).await;
        Mock::given(method("GET")).and(path(p)).respond_with(ResponseTemplate::new(200).set_body_bytes(build_chunk(info.guid, &data, true, None))).mount(&server).await;
        let c = InstallClient::with_base_url(&server.uri()).with_retry_base(Duration::from_millis(1));
        assert_eq!(c.download_chunk(&[format!("{}/base", server.uri())], "ChunksV4", &info).await.unwrap(), data);
        for req in server.received_requests().await.unwrap() {
            assert!(req.headers.get("authorization").is_none());
            assert!(req.headers.get("user-agent").unwrap().to_str().unwrap().starts_with("EpicGamesLauncher/"));
        }
    }

    #[tokio::test]
    async fn unsupported_chunk_is_not_retried() {
        let server = MockServer::start().await;
        let info = info_for([1, 2, 3, 4], b"x", 1);
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(build_chunk(info.guid, b"x", false, Some(2))))
            .expect(1)
            .mount(&server)
            .await;
        let c = InstallClient::with_base_url(&server.uri()).with_retry_base(Duration::from_millis(1));
        assert_eq!(c.download_chunk(&[server.uri()], "ChunksV4", &info).await, Err(InstallError::Unsupported));
    }

    /// A server at `/base` that answers this chunk with `res`.
    async fn chunk_server(info: &ChunkInfo, res: ResponseTemplate) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path(format!("/base/{}", chunk_path("ChunksV4", info)))).respond_with(res).mount(&server).await;
        server
    }

    fn good(info: &ChunkInfo, data: &[u8]) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_bytes(build_chunk(info.guid, data, true, None))
    }

    #[tokio::test]
    async fn a_failed_chunk_is_tried_on_the_next_server() {
        let data = b"fishy data".repeat(10);
        let info = info_for([1, 2, 3, 4], &data, 10);
        let broken = chunk_server(&info, ResponseTemplate::new(500)).await;
        let fine = chunk_server(&info, good(&info, &data)).await;
        let c = InstallClient::with_base_url(&fine.uri()).with_retry_base(Duration::from_millis(1));
        let bases = [format!("{}/base", broken.uri()), format!("{}/base", fine.uri())];
        assert_eq!(c.download_chunk(&bases, "ChunksV4", &info).await.unwrap(), data);
        assert_eq!(broken.received_requests().await.unwrap().len(), 1, "only the first try goes to the broken server");
        assert_eq!(fine.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn no_server_is_an_error() {
        let info = info_for([1, 2, 3, 4], b"x", 1);
        let c = InstallClient::with_base_url("http://127.0.0.1:9").with_retry_base(Duration::from_millis(1));
        assert!(matches!(c.download_chunk(&[], "ChunksV4", &info).await, Err(InstallError::BadManifest(_))));
    }

    #[tokio::test]
    async fn fastest_first_orders_servers_by_the_quick_test() {
        let data = b"fishy data".repeat(10);
        let info = info_for([1, 2, 3, 4], &data, 10);
        let slow = chunk_server(&info, good(&info, &data).set_delay(Duration::from_millis(400))).await;
        let fast = chunk_server(&info, good(&info, &data)).await;
        let broken = chunk_server(&info, ResponseTemplate::new(404)).await;
        let c = InstallClient::with_base_url(&fast.uri()).with_retry_base(Duration::from_millis(1));
        let b = |s: &MockServer| format!("{}/base", s.uri());
        let order = c.fastest_first(&[b(&broken), b(&slow), b(&fast)], "ChunksV4", std::slice::from_ref(&info)).await;
        assert_eq!(order, vec![b(&fast), b(&slow), b(&broken)]);
        assert_eq!(broken.received_requests().await.unwrap().len(), 1, "the quick test doesn't retry");
    }

    #[tokio::test]
    async fn one_server_needs_no_quick_test() {
        let info = info_for([1, 2, 3, 4], b"x", 1);
        let server = MockServer::start().await;
        let c = InstallClient::with_base_url(&server.uri());
        let only = vec![format!("{}/base", server.uri())];
        assert_eq!(c.fastest_first(&only, "ChunksV4", std::slice::from_ref(&info)).await, only);
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[test]
    fn host_of_keeps_only_the_server_name() {
        assert_eq!(host_of("https://download.epicgames.com/Builds/Org/o-1/abc/default"), "download.epicgames.com");
        assert_eq!(host_of("http://127.0.0.1:8080/base"), "127.0.0.1:8080");
        assert_eq!(host_of("not a url"), "?");
    }
}
