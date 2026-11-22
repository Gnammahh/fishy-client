//! Runs a download: fetches chunks (16 at a time, in order), writes files one by one, checks each
//! file's SHA-1 and notes it in resume.txt. Stopping (pause or cancel) returns `Stopped`.
//! Updates (spec §6e) also copy parts from the old files, swap changed files in through a
//! temporary file and delete removed files at the end.
use super::client::InstallClient;
use super::limit::SpeedLimiter;
use super::manifest::{FileEntry, Manifest};
use super::plan::build_update_plan;
use super::uninstall::delete_named_files;
use super::store::{file_path, InstallStore, JobInfo};
use super::{Guid, InstallError};
use futures::stream::{self, Stream, StreamExt};
use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
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

/// Limits for a run. The defaults are what the app uses; tests shrink them.
#[derive(Clone)]
pub struct RunOptions {
    /// Most bytes of shared chunks kept in memory; past this a chunk is fetched again when needed.
    pub cache_limit: usize,
    /// Free bytes on the drive of a path (swappable for tests).
    pub free_space: fn(&Path) -> std::io::Result<u64>,
    /// The download speed limit, shared with every other download (spec §6f.1). None: no limit.
    pub limiter: Option<Arc<SpeedLimiter>>,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self { cache_limit: 512 * 1024 * 1024, free_space: super::space::free_space, limiter: None }
    }
}

/// Windows "sharing violation" (32) or "lock violation" (33): another program, usually the
/// game itself, has the file open.
pub fn files_in_use(e: &std::io::Error) -> bool {
    matches!(e.raw_os_error(), Some(32 | 33))
}

/// Swapping an updated file into place. Windows refuses to replace a running program with
/// "access denied" (5), so here that also means "close the game" (spec §6f.2).
pub fn swap_error(install_path: &Path, title: &str, e: std::io::Error) -> InstallError {
    if files_in_use(&e) || e.raw_os_error() == Some(5) {
        return InstallError::FilesInUse(title.to_string());
    }
    InstallError::Io { folder: install_path.display().to_string(), detail: e.to_string() }
}

/// A write error: a file in use becomes "close the game", a full disk "Not enough space".
pub fn write_error(install_path: &Path, title: &str, e: std::io::Error, needed: u64, free: u64) -> InstallError {
    if files_in_use(&e) {
        return InstallError::FilesInUse(title.to_string());
    }
    if e.kind() == std::io::ErrorKind::StorageFull {
        return InstallError::NoSpace { drive: super::space::drive_label(install_path), needed, free };
    }
    InstallError::Io { folder: install_path.display().to_string(), detail: e.to_string() }
}

/// Shared chunks waiting for a later file, within a byte budget.
struct ChunkCache {
    chunks: HashMap<Guid, Arc<Vec<u8>>>,
    bytes: usize,
    limit: usize,
}

impl ChunkCache {
    /// Keeps `data` for later if it fits, making room by dropping others. Dropped chunks are
    /// fetched again when needed.
    fn keep(&mut self, guid: Guid, data: Arc<Vec<u8>>) {
        while self.bytes + data.len() > self.limit {
            let Some(victim) = self.chunks.keys().next().copied() else { return };
            self.forget(&victim);
        }
        self.bytes += data.len();
        self.chunks.insert(guid, data);
    }

    fn forget(&mut self, guid: &Guid) {
        if let Some(d) = self.chunks.remove(guid) {
            self.bytes -= d.len();
        }
    }
}

pub async fn run_download(
    client: &InstallClient,
    store: &InstallStore,
    job: &JobInfo,
    manifest: &Manifest,
    stop: CancellationToken,
    on_progress: impl Fn(Progress) + Send + Sync,
) -> Result<(), InstallError> {
    run_download_with(client, store, job, manifest, stop, on_progress, RunOptions::default()).await
}

pub async fn run_download_with(
    client: &InstallClient,
    store: &InstallStore,
    job: &JobInfo,
    manifest: &Manifest,
    stop: CancellationToken,
    on_progress: impl Fn(Progress) + Send + Sync,
    opts: RunOptions,
) -> Result<(), InstallError> {
    run_job(client, store, job, manifest, None, stop, on_progress, opts).await
}

/// `<name>.fishy-update` next to the file it will replace.
fn temp_path(target: &Path) -> PathBuf {
    let mut name = target.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(".fishy-update");
    target.with_file_name(name)
}

/// Reads `size` bytes at `pos` from the old file. None if it can't (missing, too short).
fn read_old(old: &mut Option<std::fs::File>, path: &Path, pos: u64, size: usize) -> Option<Vec<u8>> {
    if old.is_none() {
        *old = std::fs::File::open(path).ok();
    }
    let f = old.as_mut()?;
    f.seek(SeekFrom::Start(pos)).ok()?;
    let mut buf = vec![0u8; size];
    f.read_exact(&mut buf).ok()?;
    Some(buf)
}

/// Writes a file from freshly downloaded chunks only (the fallback when reused parts were wrong).
#[allow(clippy::too_many_arguments)]
async fn rebuild_from_downloads(
    client: &InstallClient,
    base: &str,
    dir: &'static str,
    manifest: &Manifest,
    entry: &FileEntry,
    path: &Path,
    title: &str,
    stop: &CancellationToken,
    limiter: Option<&SpeedLimiter>,
    mut on_bytes: impl FnMut(u64),
) -> Result<[u8; 20], InstallError> {
    let io = |e: std::io::Error| {
        if files_in_use(&e) {
            InstallError::FilesInUse(title.to_string())
        } else {
            InstallError::Io { folder: path.display().to_string(), detail: e.to_string() }
        }
    };
    let mut out = std::io::BufWriter::new(std::fs::File::create(path).map_err(io)?);
    let mut hasher = Sha1::new();
    let mut fetched: HashMap<Guid, Vec<u8>> = HashMap::new();
    for part in &entry.parts {
        if let std::collections::hash_map::Entry::Vacant(slot) = fetched.entry(part.guid) {
            let info = manifest.chunk(&part.guid).ok_or_else(|| InstallError::BadManifest("unknown chunk".into()))?;
            let fetch = async {
                if let Some(l) = limiter {
                    l.take(info.file_size).await;
                }
                client.download_chunk(base, dir, info).await
            };
            let data = tokio::select! {
                biased;
                _ = stop.cancelled() => return Err(InstallError::Stopped),
                d = fetch => d?,
            };
            on_bytes(info.file_size);
            slot.insert(data);
        }
        let data = &fetched[&part.guid];
        let slice = data
            .get(part.offset as usize..(part.offset + part.size) as usize)
            .ok_or_else(|| InstallError::DamagedFile(entry.filename.clone()))?;
        out.write_all(slice).map_err(io)?;
        hasher.update(slice);
    }
    out.flush().map_err(io)?;
    Ok(hasher.finalize().into())
}

/// Installs, or (with `old`, the installed version's manifest) updates. See the module docs.
#[allow(clippy::too_many_arguments)]
pub async fn run_job(
    client: &InstallClient,
    store: &InstallStore,
    job: &JobInfo,
    manifest: &Manifest,
    old: Option<&Manifest>,
    stop: CancellationToken,
    on_progress: impl Fn(Progress) + Send + Sync,
    opts: RunOptions,
) -> Result<(), InstallError> {
    let plan = build_update_plan(manifest, old, &store.finished_files(job))?;
    let base = job.base_urls.first().ok_or_else(|| InstallError::BadManifest("no CDN".into()))?.clone();
    let dir = manifest.chunk_dir();

    // Space for what's left, checked on every start and resume (spec §6c, §6e).
    let needed = plan.space_needed;
    let free = (opts.free_space)(&job.install_path).unwrap_or(u64::MAX);
    super::space::check_space(&job.install_path, needed, free)?;

    let mut downloaded = job.download_size.saturating_sub(plan.download_size);
    let total = job.download_size.max(downloaded);
    on_progress(Progress { downloaded, total });

    let mut chunks: ChunkStream = Box::pin(
        stream::iter(plan.chunk_order.clone())
            .map(|guid| {
                let info = manifest.chunk(&guid).expect("plan checked every chunk").clone();
                let base = base.clone();
                let limiter = opts.limiter.clone();
                async move {
                    if let Some(l) = &limiter {
                        l.take(info.file_size).await;
                    }
                    let result = client.download_chunk(&base, dir, &info).await;
                    (guid, info.file_size, result)
                }
            })
            .buffered(IN_FLIGHT),
    );
    let mut cache = ChunkCache { chunks: HashMap::new(), bytes: 0, limit: opts.cache_limit };
    let mut uses = plan.uses.clone();
    // Chunks already taken from the stream; if one isn't cached any more it's fetched again.
    let mut seen: std::collections::HashSet<Guid> = std::collections::HashSet::new();
    let io_err = |e: std::io::Error| write_error(&job.install_path, &job.title, e, needed, (opts.free_space)(&job.install_path).unwrap_or(0));

    for &index in &plan.files {
        if stop.is_cancelled() {
            return Err(InstallError::Stopped);
        }
        let entry: &FileEntry = &manifest.files[index];
        let target = file_path(&job.install_path, &entry.filename)?;
        // A changed file is built next to the old one and swapped in only once it's checked.
        let path = if plan.replace.contains(&index) { temp_path(&target) } else { target.clone() };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(io_err)?;
        }
        let reuse = plan.reuse.get(&index);
        let mut old_file: Option<std::fs::File> = None;
        // create() truncates: a file left half-written by an earlier run starts over.
        let mut out = std::io::BufWriter::with_capacity(1 << 20, std::fs::File::create(&path).map_err(io_err)?);
        let mut hasher = Sha1::new();
        for (n, part) in entry.parts.iter().enumerate() {
            if let Some(pos) = reuse.and_then(|r| r[n]) {
                // Copied from the old file. If it can't be read, zeros go in and the check below
                // sends the file to the rebuild.
                let bytes = read_old(&mut old_file, &target, pos, part.size as usize).unwrap_or_else(|| vec![0; part.size as usize]);
                out.write_all(&bytes).map_err(io_err)?;
                hasher.update(&bytes);
                continue;
            }
            let data = match cache.chunks.get(&part.guid) {
                Some(d) => d.clone(),
                None => {
                    let (size, data) = if seen.contains(&part.guid) {
                        // Dropped to stay under the memory limit: fetch it again.
                        let info = manifest.chunk(&part.guid).expect("plan checked every chunk");
                        let data = tokio::select! {
                            biased;
                            _ = stop.cancelled() => return Err(InstallError::Stopped),
                            d = client.download_chunk(&base, dir, info) => d?,
                        };
                        (info.file_size, data)
                    } else {
                        // The stream is in first-use order, so its next chunk is this one.
                        let next = tokio::select! {
                            biased;
                            _ = stop.cancelled() => return Err(InstallError::Stopped),
                            next = chunks.next() => next,
                        };
                        let (guid, size, data) = next.ok_or_else(|| InstallError::BadManifest("chunk stream ended".into()))?;
                        debug_assert_eq!(guid, part.guid);
                        seen.insert(guid);
                        (size, data?)
                    };
                    downloaded += size;
                    on_progress(Progress { downloaded: downloaded.min(total), total });
                    Arc::new(data)
                }
            };
            let (start, end) = (part.offset as usize, part.offset as usize + part.size as usize);
            let slice = data.get(start..end).ok_or_else(|| InstallError::DamagedFile(entry.filename.clone()))?;
            out.write_all(slice).map_err(io_err)?;
            hasher.update(slice);
            let left = uses.get_mut(&part.guid).expect("plan counted every part");
            *left -= 1;
            if *left == 0 {
                cache.forget(&part.guid);
            } else if !cache.chunks.contains_key(&part.guid) {
                cache.keep(part.guid, data);
            }
        }
        out.flush().map_err(io_err)?;
        drop(out);
        drop(old_file);
        let mut sha: [u8; 20] = hasher.finalize().into();
        if sha != entry.sha1 && reuse.is_some_and(|r| r.iter().any(Option::is_some)) {
            // The old file wasn't what the old manifest says (edited, damaged): rebuild once.
            let add = |bytes: u64| {
                downloaded += bytes;
                on_progress(Progress { downloaded: downloaded.min(total), total });
            };
            sha = rebuild_from_downloads(client, &base, dir, manifest, entry, &path, &job.title, &stop, opts.limiter.as_deref(), add).await?;
        }
        if sha != entry.sha1 {
            return Err(InstallError::DamagedFile(entry.filename.clone()));
        }
        if path != target {
            std::fs::rename(&path, &target).map_err(|e| swap_error(&job.install_path, &job.title, e))?; // replaces the old file
        }
        store.mark_finished(&job.app_name, &hex::encode(sha), &entry.filename).map_err(io_err)?;
    }
    if !plan.removed.is_empty() {
        // Best effort: a removed file that can't be deleted doesn't stop the update.
        delete_named_files(&job.install_path, plan.removed.iter().map(String::as_str), false);
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

    /// (chunk index, offset, size)
    type Part = (usize, u32, u32);

    fn fixture(chunks: &[&[u8]], files: &[(&str, &[Part])]) -> Fixture {
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
            update_from: None,
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
    async fn engine_respects_the_limiter() {
        let server = MockServer::start().await;
        let f = basic();
        serve(&server, &f, None, None).await;
        let e = env(&server, &f);
        // Twice the download size per second: the whole install should take about half a second.
        let limiter = Arc::new(super::super::limit::SpeedLimiter::new());
        limiter.set(Some(e.job.download_size * 2));
        let options = RunOptions { limiter: Some(limiter), ..RunOptions::default() };
        let start = std::time::Instant::now();
        run_job(&e.client, &e.store, &e.job, &f.manifest, None, CancellationToken::new(), |_| {}, options).await.unwrap();
        let secs = start.elapsed().as_secs_f64();
        assert!(secs >= 0.4, "finished in {secs} s, faster than the limit");
        for (name, content) in &f.expected {
            assert_eq!(&read(&e.job.install_path, name), content, "{name}");
        }
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

    fn opts(cache_limit: usize, free_space: fn(&Path) -> std::io::Result<u64>) -> RunOptions {
        RunOptions { cache_limit, free_space, limiter: None }
    }
    fn plenty(_: &Path) -> std::io::Result<u64> {
        Ok(u64::MAX)
    }

    #[tokio::test]
    async fn over_the_cache_limit_a_shared_chunk_is_fetched_again_instead_of_kept() {
        let server = MockServer::start().await;
        let shared: Vec<u8> = b"shared".repeat(50);
        let other: Vec<u8> = b"other".repeat(50);
        let f = fixture(&[&shared, &other], &[("a.txt", &[(0, 0, 100)]), ("m.txt", &[(1, 0, 250)]), ("z.txt", &[(0, 100, 200)])]);
        serve(&server, &f, None, Some((0, 2))).await; // kept nowhere, so fetched twice
        let e = env(&server, &f);
        run_download_with(&e.client, &e.store, &e.job, &f.manifest, CancellationToken::new(), |_| {}, opts(1, plenty)).await.unwrap();
        for (name, content) in &f.expected {
            assert_eq!(&read(&e.job.install_path, name), content, "{name}");
        }
    }

    #[tokio::test]
    async fn not_enough_space_stops_before_downloading() {
        let server = MockServer::start().await;
        let f = basic();
        serve(&server, &f, None, Some((0, 0))).await;
        let e = env(&server, &f);
        fn tiny(_: &Path) -> std::io::Result<u64> {
            Ok(10)
        }
        let r = run_download_with(&e.client, &e.store, &e.job, &f.manifest, CancellationToken::new(), |_| {}, opts(1 << 20, tiny)).await;
        assert_eq!(r, Err(InstallError::NoSpace { drive: crate::install::space::drive_label(&e.job.install_path), needed: 3900, free: 10 }));
    }

    #[tokio::test]
    async fn resume_needs_space_only_for_unfinished_files() {
        let server = MockServer::start().await;
        let ca: Vec<u8> = b"aaaa".repeat(10);
        let cb: Vec<u8> = b"bbbb".repeat(10);
        let f = fixture(&[&ca, &cb], &[("a.txt", &[(0, 0, 40)]), ("b.txt", &[(1, 0, 40)])]);
        serve(&server, &f, None, None).await;
        let e = env(&server, &f);
        std::fs::create_dir_all(&e.job.install_path).unwrap();
        std::fs::write(e.job.install_path.join("a.txt"), &f.expected[0].1).unwrap();
        e.store.mark_finished("Tide", "x", "a.txt").unwrap();
        fn exactly_b(_: &Path) -> std::io::Result<u64> {
            Ok(40)
        }
        run_download_with(&e.client, &e.store, &e.job, &f.manifest, CancellationToken::new(), |_| {}, opts(1 << 20, exactly_b)).await.unwrap();
    }

    #[test]
    fn replacing_a_file_that_is_locked_says_close_the_game() {
        let p = Path::new(r"C:\Games\Bendy");
        for code in [5, 32, 33] {
            assert_eq!(swap_error(p, "Bendy", std::io::Error::from_raw_os_error(code)), InstallError::FilesInUse("Bendy".into()), "{code}");
        }
        assert!(matches!(swap_error(p, "Bendy", std::io::Error::from_raw_os_error(3)), InstallError::Io { .. }));
    }

    #[cfg(windows)]
    #[test]
    fn a_running_program_blocks_the_swap_and_says_close_the_game() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("Game.exe");
        let fresh = dir.path().join("Game.exe.fishy-update");
        std::fs::write(&target, b"old").unwrap();
        std::fs::write(&fresh, b"new").unwrap();
        // How Windows holds a running program: others may read it, not replace or delete it.
        let running = std::fs::OpenOptions::new().read(true).share_mode(1).open(&target).unwrap();
        let e = std::fs::rename(&fresh, &target).unwrap_err();
        drop(running);
        assert_eq!(swap_error(dir.path(), "Bendy", e), InstallError::FilesInUse("Bendy".into()));
    }

    #[test]
    fn a_file_in_use_says_close_the_game() {
        let p = Path::new(r"C:\Games\Bendy");
        for code in [32, 33] {
            assert_eq!(write_error(p, "Bendy", std::io::Error::from_raw_os_error(code), 0, 0), InstallError::FilesInUse("Bendy".into()));
        }
    }

    #[test]
    fn access_denied_still_names_the_folder() {
        let p = Path::new(r"C:\Games\Bendy");
        assert!(matches!(write_error(p, "Bendy", std::io::Error::from_raw_os_error(5), 0, 0), InstallError::Io { .. }));
    }

    #[test]
    fn a_full_disk_is_reported_as_no_space() {
        let p = Path::new(r"C:\Games\Tide");
        let full = std::io::Error::from(std::io::ErrorKind::StorageFull);
        assert_eq!(write_error(p, "Fish", full, 500, 3), InstallError::NoSpace { drive: "C:".into(), needed: 500, free: 3 });
        let denied = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        assert!(matches!(write_error(p, "Fish", denied, 500, 3), InstallError::Io { .. }));
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
    // ---- Updates (spec §6e) ----

    /// Old and new versions built from the same three chunks, so chunk ids match.
    fn versions() -> (Fixture, Fixture) {
        let c0: Vec<u8> = (0..=255u8).cycle().take(3000).collect();
        let c1: Vec<u8> = b"old!".repeat(100);
        let c2: Vec<u8> = b"new!".repeat(100);
        let chunks: [&[u8]; 3] = [&c0, &c1, &c2];
        let old = fixture(&chunks, &[("same.txt", &[(1, 0, 100)]), ("big.pak", &[(0, 0, 3000)]), ("Old/gone.txt", &[(1, 100, 50)])]);
        let new = fixture(&chunks, &[
            ("same.txt", &[(1, 0, 100)]),
            ("big.pak", &[(0, 0, 2000), (2, 0, 200), (0, 2000, 1000)]),
            ("added.txt", &[(2, 200, 100)]),
        ]);
        (old, new)
    }

    /// Mounts every chunk of `f`, expecting `counts[i]` downloads of chunk i (None: any).
    async fn serve_counted(server: &MockServer, f: &Fixture, counts: &[Option<u64>]) {
        for (i, info) in f.manifest.chunks.iter().enumerate() {
            let res = ResponseTemplate::new(200).set_body_bytes(build_chunk(info.guid, &f.chunk_data[i], true, None));
            let mock = Mock::given(method("GET")).and(path(format!("/base/{}", chunk_path("ChunksV4", info)))).respond_with(res);
            match counts.get(i).copied().flatten() {
                Some(n) => mock.expect(n).mount(server).await,
                None => mock.mount(server).await,
            }
        }
    }

    /// The old version, already installed on disk, and an update job to the new one.
    fn installed_old(server: &MockServer, old: &Fixture, new: &Fixture) -> Env {
        let mut e = env(server, new);
        for (name, content) in &old.expected {
            let p = file_path(&e.job.install_path, name).unwrap();
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, content).unwrap();
        }
        e.job.update_from = Some("1".into());
        e
    }

    fn write_new(e: &Env, new: &Fixture, name: &str) {
        let content = &new.expected.iter().find(|(n, _)| n == name).unwrap().1;
        std::fs::write(file_path(&e.job.install_path, name).unwrap(), content).unwrap();
        e.store.mark_finished("Tide", "x", name).unwrap();
    }

    fn assert_new_version(e: &Env, new: &Fixture) {
        for (name, content) in &new.expected {
            assert_eq!(&read(&e.job.install_path, name), content, "{name}");
        }
        assert!(!e.job.install_path.join("Old").exists(), "the removed file and its empty folder are gone");
        let leftovers: Vec<_> = std::fs::read_dir(&e.job.install_path)
            .unwrap()
            .flatten()
            .filter(|f| f.file_name().to_string_lossy().ends_with(".fishy-update"))
            .collect();
        assert!(leftovers.is_empty(), "no temporary files left");
    }

    #[tokio::test]
    async fn update_skips_unchanged_files_and_never_downloads_reused_chunks() {
        let server = MockServer::start().await;
        let (old, new) = versions();
        serve_counted(&server, &new, &[Some(0), Some(0), Some(1)]).await;
        let e = installed_old(&server, &old, &new);
        let same = e.job.install_path.join("same.txt");
        let before = std::fs::metadata(&same).unwrap().modified().unwrap();
        std::thread::sleep(Duration::from_millis(20));
        run_job(&e.client, &e.store, &e.job, &new.manifest, Some(&old.manifest), CancellationToken::new(), |_| {}, RunOptions::default())
            .await
            .unwrap();
        assert_new_version(&e, &new);
        assert_eq!(std::fs::metadata(&same).unwrap().modified().unwrap(), before, "unchanged file is never rewritten");
    }

    #[tokio::test]
    async fn update_rebuilds_a_file_whose_old_copy_was_edited() {
        let server = MockServer::start().await;
        let (old, new) = versions();
        serve_counted(&server, &new, &[None, Some(0), None]).await;
        let e = installed_old(&server, &old, &new);
        std::fs::write(e.job.install_path.join("big.pak"), vec![7u8; 3000]).unwrap(); // the user changed it
        run_job(&e.client, &e.store, &e.job, &new.manifest, Some(&old.manifest), CancellationToken::new(), |_| {}, RunOptions::default())
            .await
            .unwrap();
        assert_new_version(&e, &new);
    }

    #[tokio::test]
    async fn update_paused_then_resumed_finishes() {
        let server = MockServer::start().await;
        let (old, new) = versions();
        serve_counted(&server, &new, &[Some(0), Some(0), Some(1)]).await;
        let e = installed_old(&server, &old, &new);
        write_new(&e, &new, "added.txt"); // done before the pause
        std::fs::write(e.job.install_path.join("big.pak.fishy-update"), b"half").unwrap(); // half-written temp
        run_job(&e.client, &e.store, &e.job, &new.manifest, Some(&old.manifest), CancellationToken::new(), |_| {}, RunOptions::default())
            .await
            .unwrap();
        assert_new_version(&e, &new);
    }

    #[tokio::test]
    async fn resume_after_all_files_still_deletes_removed_files() {
        let server = MockServer::start().await;
        let (old, new) = versions();
        serve_counted(&server, &new, &[Some(0), Some(0), Some(0)]).await;
        let e = installed_old(&server, &old, &new);
        write_new(&e, &new, "added.txt");
        write_new(&e, &new, "big.pak");
        run_job(&e.client, &e.store, &e.job, &new.manifest, Some(&old.manifest), CancellationToken::new(), |_| {}, RunOptions::default())
            .await
            .unwrap();
        assert_new_version(&e, &new);
    }

    #[tokio::test]
    async fn update_space_check_uses_the_update_plan() {
        let server = MockServer::start().await;
        let (old, new) = versions();
        serve_counted(&server, &new, &[Some(0), Some(0), Some(0)]).await;
        let e = installed_old(&server, &old, &new);
        fn tiny(_: &Path) -> std::io::Result<u64> {
            Ok(10)
        }
        let r = run_job(&e.client, &e.store, &e.job, &new.manifest, Some(&old.manifest), CancellationToken::new(), |_| {}, opts(1 << 20, tiny)).await;
        // added 100 + growth 200 + largest changed 3200
        assert_eq!(r, Err(InstallError::NoSpace { drive: crate::install::space::drive_label(&e.job.install_path), needed: 3500, free: 10 }));
    }
}
