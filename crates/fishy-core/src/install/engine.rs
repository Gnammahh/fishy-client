//! Runs a download: fetches chunks (16 at a time, in order), writes files one by one, checks each
//! file's SHA-1 and notes it in resume.txt. Stopping (pause or cancel) returns `Stopped`.
use super::client::InstallClient;
use super::manifest::{FileEntry, Manifest};
use super::plan::build_plan;
use super::store::{file_path, InstallStore, JobInfo};
use super::{Guid, InstallError};
use futures::stream::{self, Stream, StreamExt};
use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::io::Write;
use std::pin::Pin;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Chunks downloading at once (spec §6c).
const IN_FLIGHT: usize = 16;

type ChunkStream<'a> = Pin<Box<dyn Stream<Item = (Guid, u64, Result<Vec<u8>, InstallError>)> + Send + 'a>>;

/// Compressed bytes. `total` is the whole game's download size, so a resumed bar starts where it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub downloaded: u64,
    pub total: u64,
}

pub async fn run_download(
    client: &InstallClient,
    store: &InstallStore,
    job: &JobInfo,
    manifest: &Manifest,
    stop: CancellationToken,
    on_progress: impl Fn(Progress) + Send + Sync,
) -> Result<(), InstallError> {
    let plan = build_plan(manifest, &store.finished_files(job))?;
    let base = job.base_urls.first().ok_or_else(|| InstallError::BadManifest("no CDN".into()))?.clone();
    let dir = manifest.chunk_dir();
    let mut downloaded = job.download_size.saturating_sub(plan.download_size);
    let total = job.download_size.max(downloaded);
    on_progress(Progress { downloaded, total });

    let mut chunks: ChunkStream = Box::pin(
        stream::iter(plan.chunk_order.clone())
            .map(|guid| {
                let info = manifest.chunk(&guid).expect("plan checked every chunk").clone();
                let base = base.clone();
                async move {
                    let result = client.download_chunk(&base, dir, &info).await;
                    (guid, info.file_size, result)
                }
            })
            .buffered(IN_FLIGHT),
    );
    let mut cache: HashMap<Guid, Arc<Vec<u8>>> = HashMap::new();
    let mut uses = plan.uses.clone();
    let io_err = |e: std::io::Error| InstallError::Io { folder: job.install_path.display().to_string(), detail: e.to_string() };

    for &index in &plan.files {
        if stop.is_cancelled() {
            return Err(InstallError::Stopped);
        }
        let entry: &FileEntry = &manifest.files[index];
        let path = file_path(&job.install_path, &entry.filename)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(io_err)?;
        }
        // create() truncates: a file left half-written by an earlier run starts over.
        let mut out = std::io::BufWriter::with_capacity(1 << 20, std::fs::File::create(&path).map_err(io_err)?);
        let mut hasher = Sha1::new();
        for part in &entry.parts {
            while !cache.contains_key(&part.guid) {
                let next = tokio::select! {
                    biased;
                    _ = stop.cancelled() => return Err(InstallError::Stopped),
                    next = chunks.next() => next,
                };
                let (guid, size, data) = next.ok_or_else(|| InstallError::BadManifest("chunk stream ended".into()))?;
                cache.insert(guid, Arc::new(data?));
                downloaded += size;
                on_progress(Progress { downloaded, total });
            }
            let data = cache[&part.guid].clone();
            let (start, end) = (part.offset as usize, part.offset as usize + part.size as usize);
            let slice = data.get(start..end).ok_or_else(|| InstallError::DamagedFile(entry.filename.clone()))?;
            out.write_all(slice).map_err(io_err)?;
            hasher.update(slice);
            let left = uses.get_mut(&part.guid).expect("plan counted every part");
            *left -= 1;
            if *left == 0 {
                cache.remove(&part.guid);
            }
        }
        out.flush().map_err(io_err)?;
        drop(out);
        let sha: [u8; 20] = hasher.finalize().into();
        if sha != entry.sha1 {
            return Err(InstallError::DamagedFile(entry.filename.clone()));
        }
        store.mark_finished(&job.app_name, &hex::encode(sha), &entry.filename).map_err(io_err)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::chunk::{build_chunk, chunk_path, info_for};
    use crate::install::manifest::testbuild::build_binary;
    use crate::install::manifest::{ChunkInfo, ChunkPart, FileEntry, ManifestMeta};
    use crate::install::Guid;
    use sha1::{Digest, Sha1};
    use std::path::Path;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Chunk contents, and files as (name, [(chunk index, offset, size)]).
    struct Fixture {
        manifest: Manifest,
        chunk_data: Vec<Vec<u8>>,
        expected: Vec<(String, Vec<u8>)>,
    }

    fn fixture(chunks: &[&[u8]], files: &[(&str, &[(usize, u32, u32)])]) -> Fixture {
        let guid = |i: usize| -> Guid { [i as u32 + 1, 0xAA, 0xBB, 0xCC] };
        let infos: Vec<ChunkInfo> = chunks.iter().enumerate().map(|(i, d)| info_for(guid(i), d, 100 + i as u64)).collect();
        let mut entries = Vec::new();
        let mut expected = Vec::new();
        for (name, parts) in files {
            let mut content = Vec::new();
            for &(c, off, size) in *parts {
                content.extend(&chunks[c][off as usize..(off + size) as usize]);
            }
            entries.push(FileEntry {
                filename: name.to_string(),
                sha1: Sha1::digest(&content).into(),
                executable: false,
                parts: parts.iter().map(|&(c, offset, size)| ChunkPart { guid: guid(c), offset, size }).collect(),
            });
            expected.push((name.to_string(), content));
        }
        let meta = ManifestMeta { feature_level: 18, app_name: "Tide".into(), build_version: "1".into(), ..Default::default() };
        Fixture { manifest: Manifest::new(meta, infos, entries), chunk_data: chunks.iter().map(|c| c.to_vec()).collect(), expected }
    }

    async fn serve(server: &MockServer, f: &Fixture, delay: Option<Duration>, expect: Option<(usize, u64)>) {
        for (i, info) in f.manifest.chunks.iter().enumerate() {
            let mut res = ResponseTemplate::new(200).set_body_bytes(build_chunk(info.guid, &f.chunk_data[i], true, None));
            if let Some(d) = delay {
                res = res.set_delay(d);
            }
            let mock = Mock::given(method("GET")).and(path(format!("/base/{}", chunk_path("ChunksV4", info)))).respond_with(res);
            let mock = match expect {
                Some((idx, n)) if idx == i => mock.expect(n),
                _ => mock,
            };
            mock.mount(server).await;
        }
    }

    struct Env {
        _dir: tempfile::TempDir,
        store: InstallStore,
        job: JobInfo,
        client: InstallClient,
    }

    fn env(server: &MockServer, f: &Fixture) -> Env {
        let dir = tempfile::tempdir().unwrap();
        let store = InstallStore::at(dir.path().join("data"));
        let job = JobInfo {
            app_name: "Tide".into(),
            title: "Tidepool".into(),
            namespace: "ns".into(),
            catalog_item_id: "cat".into(),
            install_path: dir.path().join("Games").join("Tidepool"),
            download_size: f.manifest.chunks.iter().map(|c| c.file_size).sum(),
            install_size: f.manifest.install_size(),
            base_urls: vec![format!("{}/base", server.uri())],
        };
        store.save_job(&job, &build_binary(&f.manifest, true)).unwrap();
        Env { _dir: dir, store, job, client: InstallClient::with_base_url(&server.uri()).with_retry_base(Duration::from_millis(1)) }
    }

    fn read(root: &Path, name: &str) -> Vec<u8> {
        std::fs::read(crate::install::store::file_path(root, name).unwrap()).unwrap()
    }

    fn basic() -> Fixture {
        let c0: Vec<u8> = (0..=255u8).cycle().take(3000).collect();
        let c1: Vec<u8> = b"tidepool!".repeat(100);
        fixture(&[&c0, &c1], &[
            ("Binaries/Win64/Tide.exe", &[(0, 0, 2000), (1, 0, 900)]),
            ("readme.txt", &[(0, 2000, 1000)]),
            ("Saves/empty.cfg", &[]),
        ])
    }

    #[tokio::test]
    async fn installs_all_files_with_correct_content() {
        let server = MockServer::start().await;
        let f = basic();
        serve(&server, &f, None, None).await;
        let e = env(&server, &f);
        run_download(&e.client, &e.store, &e.job, &f.manifest, CancellationToken::new(), |_| {}).await.unwrap();
        for (name, content) in &f.expected {
            assert_eq!(&read(&e.job.install_path, name), content, "{name}");
        }
        let finished = e.store.finished_files(&e.job);
        assert_eq!(finished.len(), 3);
    }

    #[tokio::test]
    async fn shared_chunk_downloaded_once() {
        let server = MockServer::start().await;
        let shared: Vec<u8> = b"shared".repeat(50);
        let other: Vec<u8> = b"other".repeat(50);
        let f = fixture(&[&shared, &other], &[
            ("a.txt", &[(0, 0, 100)]),
            ("m.txt", &[(1, 0, 250)]),
            ("z.txt", &[(0, 100, 200)]),
        ]);
        serve(&server, &f, None, Some((0, 1))).await;
        let e = env(&server, &f);
        run_download(&e.client, &e.store, &e.job, &f.manifest, CancellationToken::new(), |_| {}).await.unwrap();
        assert_eq!(read(&e.job.install_path, "z.txt"), f.expected[2].1);
        assert_eq!(read(&e.job.install_path, "a.txt"), f.expected[0].1);
    }

    #[tokio::test]
    async fn resume_rewrites_unfinished_file() {
        let server = MockServer::start().await;
        let ca: Vec<u8> = b"aaaa".repeat(10);
        let cb: Vec<u8> = b"bbbb".repeat(10);
        let f = fixture(&[&ca, &cb], &[("a.txt", &[(0, 0, 40)]), ("b.txt", &[(1, 0, 40)])]);
        serve(&server, &f, None, Some((0, 0))).await; // a.txt's chunk must never be fetched
        let e = env(&server, &f);
        std::fs::create_dir_all(&e.job.install_path).unwrap();
        std::fs::write(e.job.install_path.join("a.txt"), &f.expected[0].1).unwrap();
        e.store.mark_finished("Tide", "x", "a.txt").unwrap();
        std::fs::write(e.job.install_path.join("b.txt"), vec![b'!'; 500]).unwrap(); // half-written junk, longer than the real file
        run_download(&e.client, &e.store, &e.job, &f.manifest, CancellationToken::new(), |_| {}).await.unwrap();
        assert_eq!(read(&e.job.install_path, "b.txt"), f.expected[1].1);
    }

    #[tokio::test]
    async fn damaged_file_is_caught() {
        let server = MockServer::start().await;
        let mut f = basic();
        f.manifest.files[1].sha1[0] ^= 0xFF; // readme.txt's expected hash is now wrong
        serve(&server, &f, None, None).await;
        let e = env(&server, &f);
        let r = run_download(&e.client, &e.store, &e.job, &f.manifest, CancellationToken::new(), |_| {}).await;
        assert_eq!(r, Err(InstallError::DamagedFile("readme.txt".into())));
        assert!(!e.store.finished_files(&e.job).contains("readme.txt"));
    }

    #[tokio::test]
    async fn stop_returns_stopped_and_keeps_resume() {
        let server = MockServer::start().await;
        let f = basic();
        serve(&server, &f, Some(Duration::from_secs(5)), None).await;
        let e = env(&server, &f);
        e.store.mark_finished("Tide", "x", "Saves/empty.cfg").unwrap();
        let stop = CancellationToken::new();
        let s2 = stop.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            s2.cancel();
        });
        let started = std::time::Instant::now();
        let r = run_download(&e.client, &e.store, &e.job, &f.manifest, stop, |_| {}).await;
        assert_eq!(r, Err(InstallError::Stopped));
        assert!(started.elapsed() < Duration::from_secs(3), "stopping must not wait for downloads");
        let resume = std::fs::read_to_string(e.store_root().join("downloads/Tide/resume.txt")).unwrap();
        assert_eq!(resume, "x:Saves/empty.cfg\n");
    }

    #[tokio::test]
    async fn progress_reaches_download_size() {
        let server = MockServer::start().await;
        let f = basic();
        serve(&server, &f, None, None).await;
        let e = env(&server, &f);
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s = seen.clone();
        run_download(&e.client, &e.store, &e.job, &f.manifest, CancellationToken::new(), move |p| s.lock().unwrap().push(p)).await.unwrap();
        let seen = seen.lock().unwrap();
        assert_eq!(seen.first().unwrap().downloaded, 0);
        assert_eq!(*seen.last().unwrap(), Progress { downloaded: 201, total: 201 });
        assert!(seen.windows(2).all(|w| w[0].downloaded <= w[1].downloaded));
    }

    #[tokio::test]
    async fn bad_base_url_is_an_error() {
        let f = basic();
        let server = MockServer::start().await;
        let mut e = env(&server, &f);
        e.job.base_urls.clear();
        let r = run_download(&e.client, &e.store, &e.job, &f.manifest, CancellationToken::new(), |_| {}).await;
        assert!(matches!(r, Err(InstallError::BadManifest(_))));
    }

    impl Env {
        fn store_root(&self) -> std::path::PathBuf {
            self._dir.path().join("data")
        }
    }
}
