//! The Downloads manager: one game downloads at a time, the rest wait in a queue (spec §6c).
//! It owns a stop token per running job and tells the UI about changes with `downloads-changed`.
use fishy_core::install::engine::{run_job, Progress, RunOptions};
use fishy_core::install::manifest::{parse_manifest, Manifest};
use fishy_core::install::speed::SpeedMeter;
use fishy_core::install::store::{InstallStore, JobInfo};
use fishy_core::install::uninstall::discard_download;
use fishy_core::install::client::InstallClient;
use fishy_core::install::limit::SpeedLimiter;
use fishy_core::install::InstallError;
use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

/// Shown when Cancel or the queue's × is used on an update (spec §6e).
pub const UPDATE_NO_CANCEL: &str = "Updates can be paused but not cancelled.";

/// Progress events are sent at most this often (spec: about 4 times a second).
const EMIT_EVERY: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    Downloading,
    Queued,
    Paused,
    Error,
    /// Cancelled; its files are being deleted. Hidden, but the game stays busy until done.
    Removing,
}

impl JobState {
    fn label(self) -> &'static str {
        match self {
            Self::Downloading => "downloading",
            Self::Queued => "queued",
            Self::Paused => "paused",
            Self::Error => "error",
            Self::Removing => "removing",
        }
    }
}

pub struct Job {
    pub info: JobInfo,
    pub manifest: Arc<Manifest>,
    /// For an update: the installed version's manifest.
    pub old: Option<Arc<Manifest>>,
    pub state: JobState,
    pub downloaded: u64,
    pub bytes_per_sec: f64,
    pub error: Option<String>,
    stop: Option<CancellationToken>,
    cancel_requested: bool,
}

impl Job {
    pub fn new(info: JobInfo, manifest: Arc<Manifest>, state: JobState) -> Self {
        Self { info, manifest, old: None, state, downloaded: 0, bytes_per_sec: 0.0, error: None, stop: None, cancel_requested: false }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DownloadItem {
    pub app_name: String,
    pub title: String,
    pub state: &'static str,
    pub downloaded: u64,
    pub total: u64,
    pub bytes_per_sec: f64,
    pub error: Option<String>,
    /// An update (can be paused, not cancelled).
    pub update: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DownloadsView {
    pub items: Vec<DownloadItem>,
}

#[derive(Default)]
pub struct Inner {
    pub jobs: Vec<Job>,
    /// Delete all my data is running: nothing may start or join the queue.
    pub wiping: bool,
}

impl Inner {
    /// Bytes the other downloads still have to write, with their folders (spec §6c space check).
    pub fn left_to_write(&self, except: &str) -> Vec<(std::path::PathBuf, u64)> {
        self.jobs
            .iter()
            .filter(|j| j.info.app_name != except && !matches!(j.state, JobState::Removing | JobState::Error))
            .map(|j| {
                let to_download = j.info.download_size.saturating_sub(j.downloaded);
                let bytes = if j.info.update_from.is_some() || j.info.download_size == 0 {
                    to_download
                } else {
                    // An install writes install_size in all, in step with what's downloaded.
                    (j.info.install_size as u128 * to_download as u128 / j.info.download_size as u128) as u64
                };
                (j.info.install_path.clone(), bytes)
            })
            .collect()
    }

    /// Running job first, then the queue in order, then paused and failed ones.
    /// Something is still writing (a running download, or a cancel deleting its files).
    pub fn busy(&self) -> bool {
        self.jobs.iter().any(|j| matches!(j.state, JobState::Downloading | JobState::Removing))
    }

    /// Asks the running download to stop and pauses the queue.
    pub fn stop_all(&mut self) {
        for job in &mut self.jobs {
            match job.state {
                JobState::Downloading => job.stop.iter().for_each(CancellationToken::cancel),
                JobState::Queued => job.state = JobState::Paused,
                _ => {}
            }
        }
    }

    /// Empties the queue (Delete all my data).
    pub fn take_all(&mut self) -> Vec<JobInfo> {
        self.jobs.drain(..).map(|j| j.info).collect()
    }

    pub fn view(&self) -> DownloadsView {
        let rank = |s: JobState| match s {
            JobState::Downloading => 0,
            JobState::Queued => 1,
            JobState::Paused | JobState::Error | JobState::Removing => 2,
        };
        let mut jobs: Vec<&Job> = self.jobs.iter().filter(|j| j.state != JobState::Removing).collect();
        jobs.sort_by_key(|j| rank(j.state)); // stable: keeps queue order
        DownloadsView {
            items: jobs
                .into_iter()
                .map(|j| DownloadItem {
                    app_name: j.info.app_name.clone(),
                    title: j.info.title.clone(),
                    state: j.state.label(),
                    downloaded: j.downloaded,
                    total: j.info.download_size,
                    bytes_per_sec: if j.state == JobState::Downloading { j.bytes_per_sec } else { 0.0 },
                    error: j.error.clone(),
                    update: j.info.update_from.is_some(),
                })
                .collect(),
        }
    }

    /// The job to start now: the first queued one, if nothing is running.
    pub fn next_to_start(&self) -> Option<usize> {
        if self.wiping || self.jobs.iter().any(|j| j.state == JobState::Downloading) {
            return None;
        }
        self.jobs.iter().position(|j| j.state == JobState::Queued)
    }

    pub fn find(&self, app: &str) -> Option<usize> {
        self.jobs.iter().position(|j| j.info.app_name == app)
    }

    /// Cancel, step 1. A running job is asked to stop (its task deletes the files); any other
    /// job is marked Removing and returned so the caller deletes its files.
    pub fn begin_removal(&mut self, app: &str) -> Option<JobInfo> {
        let i = self.find(app)?;
        let job = &mut self.jobs[i];
        if job.info.update_from.is_some() {
            return None; // updates can't be cancelled (spec §6e)
        }
        match job.state {
            JobState::Removing => None,
            JobState::Downloading => {
                job.cancel_requested = true;
                job.stop.iter().for_each(CancellationToken::cancel);
                None
            }
            _ => {
                job.state = JobState::Removing;
                Some(job.info.clone())
            }
        }
    }

    pub fn is_update(&self, app: &str) -> bool {
        self.find(app).is_some_and(|i| self.jobs[i].info.update_from.is_some())
    }

    /// Uninstall of a game with a paused (or failed) update: the job leaves the list.
    /// False if the update is running or queued.
    pub fn take_stopped_update(&mut self, app: &str) -> bool {
        let Some(i) = self.find(app) else { return true };
        let job = &self.jobs[i];
        if job.info.update_from.is_none() || !matches!(job.state, JobState::Paused | JobState::Error) {
            return false;
        }
        self.jobs.remove(i);
        true
    }

    /// Cancel, step 2: gone when the files are deleted, or back as an error with the reason.
    pub fn end_removal(&mut self, app: &str, result: Result<(), String>) {
        let Some(i) = self.find(app) else { return };
        match result {
            Ok(()) => {
                self.jobs.remove(i);
            }
            Err(message) => {
                let job = &mut self.jobs[i];
                job.state = JobState::Error;
                job.error = Some(message);
                job.stop = None;
                job.cancel_requested = false;
            }
        }
    }
}

/// Shared by every command and every running download task.
#[derive(Clone)]
pub struct Downloads {
    pub store: Arc<InstallStore>,
    pub client: InstallClient,
    /// The speed limit every download shares (spec §6f.1); `settings_set` changes it live.
    pub limiter: Arc<SpeedLimiter>,
    inner: Arc<Mutex<Inner>>,
}

impl Downloads {
    /// Downloads left from last time come back as Paused.
    pub fn new(store: InstallStore, client: InstallClient) -> Self {
        let jobs = store
            .pending_jobs()
            .into_iter()
            .filter_map(|info| {
                let bytes = store.job_manifest(&info.app_name).ok()?;
                let manifest = parse_manifest(&bytes).ok()?;
                let mut job = Job::new(info, Arc::new(manifest), JobState::Paused);
                if job.info.update_from.is_some() {
                    // Without the old manifest the update still works, it just downloads everything.
                    job.old = store.installed_manifest(&job.info.app_name).ok().and_then(|b| parse_manifest(&b).ok()).map(Arc::new);
                }
                job.downloaded = already_downloaded(&store, &job.info, &job.manifest, job.old.as_deref());
                Some(job)
            })
            .collect();
        Self { store: Arc::new(store), client, limiter: Arc::new(SpeedLimiter::new()), inner: Arc::new(Mutex::new(Inner { jobs, wiping: false })) }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn view(&self) -> DownloadsView {
        self.lock().view()
    }

    /// See `Inner::left_to_write`.
    pub fn left_to_write(&self, except: &str) -> Vec<(std::path::PathBuf, u64)> {
        self.lock().left_to_write(except)
    }

    /// Folders taken by installed games and other downloads.
    pub fn taken_folders(&self, except: &str) -> Vec<std::path::PathBuf> {
        let mut taken: Vec<_> = self.store.installed().into_iter().filter(|g| g.app_name != except).map(|g| g.install_path).collect();
        taken.extend(self.lock().jobs.iter().filter(|j| j.info.app_name != except).map(|j| j.info.install_path.clone()));
        taken
    }

    pub fn has_job(&self, app: &str) -> bool {
        self.lock().find(app).is_some()
    }

    fn emit(&self, app: &AppHandle) {
        let _ = app.emit("downloads-changed", self.view());
    }

    /// Adds a new download to the queue (or starts it).
    pub fn enqueue(&self, app: &AppHandle, info: JobInfo, manifest: Arc<Manifest>, old: Option<Arc<Manifest>>) {
        {
            let mut inner = self.lock();
            if inner.wiping || inner.find(&info.app_name).is_some() {
                return;
            }
            let mut job = Job::new(info, manifest, JobState::Queued);
            job.old = old;
            // An imported repair starts with most files already there (spec §6g).
            job.downloaded = already_downloaded(&self.store, &job.info, &job.manifest, job.old.as_deref());
            inner.jobs.push(job);
        }
        self.pump(app);
    }

    pub fn pause(&self, app: &AppHandle, app_name: &str) {
        tracing::info!("pause {app_name}");
        {
            let mut inner = self.lock();
            let Some(i) = inner.find(app_name) else { return };
            let job = &mut inner.jobs[i];
            match job.state {
                // The task notices, stops and marks it Paused.
                JobState::Downloading => job.stop.iter().for_each(CancellationToken::cancel),
                JobState::Queued => job.state = JobState::Paused,
                _ => {}
            }
        }
        self.emit(app);
    }

    /// Resume, or Retry after an error.
    pub fn resume(&self, app: &AppHandle, app_name: &str) {
        tracing::info!("resume {app_name}");
        {
            let mut inner = self.lock();
            let Some(i) = inner.find(app_name) else { return };
            let job = &mut inner.jobs[i];
            if matches!(job.state, JobState::Paused | JobState::Error) {
                job.state = JobState::Queued;
                job.error = None;
            }
        }
        self.pump(app);
    }

    /// Stops the download and deletes what it wrote.
    pub async fn cancel(&self, app: &AppHandle, app_name: &str) -> Result<(), String> {
        if self.lock().is_update(app_name) {
            return Err(UPDATE_NO_CANCEL.to_string());
        }
        // A running job's own task deletes the files once it has stopped writing.
        tracing::info!("cancel {app_name}");
        let Some(info) = self.lock().begin_removal(app_name) else {
            self.emit(app);
            return Ok(());
        };
        self.emit(app);
        let result = self.discard(info).await;
        self.lock().end_removal(app_name, result.clone());
        self.emit(app);
        result
    }

    async fn discard(&self, info: JobInfo) -> Result<(), String> {
        let store = self.store.clone();
        tauri::async_runtime::spawn_blocking(move || discard_download(&store, &info))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.user_message())
    }

    /// For Uninstall: drops a paused or failed update of this game. False if one is running or queued.
    pub fn take_stopped_update(&self, app_name: &str) -> bool {
        self.lock().take_stopped_update(app_name)
    }

    /// Log out: the running download pauses and nothing new starts.
    pub fn pause_all(&self, app: &AppHandle) {
        self.lock().stop_all();
        self.emit(app);
    }

    /// Delete all my data, step 1: nothing new may start, everything stops, and once nothing is
    /// writing every job is taken off the queue. If something is still writing after `timeout`,
    /// it gives up and leaves everything as it was (Err), so no file can come back after a wipe.
    /// Call [`Self::end_wipe`] when the wipe is over.
    pub async fn stop_for_wipe(&self, timeout: Duration) -> Result<Vec<JobInfo>, ()> {
        self.lock().wiping = true;
        let deadline = Instant::now() + timeout;
        loop {
            {
                let mut inner = self.lock();
                inner.stop_all(); // again each time, in case something slipped in
                if !inner.busy() {
                    return Ok(inner.take_all());
                }
            }
            if Instant::now() >= deadline {
                self.lock().wiping = false;
                return Err(());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// The wipe is over (or was refused): downloads may start again.
    pub fn end_wipe(&self) {
        self.lock().wiping = false;
    }

    /// Starts the next queued job if nothing is running.
    fn pump(&self, app: &AppHandle) {
        let started = {
            let mut inner = self.lock();
            inner.next_to_start().map(|i| {
                let job = &mut inner.jobs[i];
                let stop = CancellationToken::new();
                job.state = JobState::Downloading;
                job.stop = Some(stop.clone());
                job.cancel_requested = false;
                (job.info.clone(), job.manifest.clone(), job.old.clone(), stop)
            })
        };
        self.emit(app);
        if let Some((info, manifest, old, stop)) = started {
            let this = self.clone();
            let app = app.clone();
            tauri::async_runtime::spawn(async move { this.run(app, info, manifest, old, stop).await });
        }
    }

    async fn run(self, app: AppHandle, info: JobInfo, manifest: Arc<Manifest>, old: Option<Arc<Manifest>>, stop: CancellationToken) {
        let meter = Mutex::new((SpeedMeter::new(), Instant::now() - EMIT_EVERY));
        let on_progress = |p: Progress| {
            let speed = {
                let mut m = meter.lock().unwrap_or_else(|e| e.into_inner());
                let now = Instant::now();
                m.0.add(now, p.downloaded);
                let due = now.duration_since(m.1) >= EMIT_EVERY || p.downloaded >= p.total;
                if due {
                    m.1 = now;
                }
                due.then(|| m.0.bytes_per_sec())
            };
            {
                let mut inner = self.lock();
                if let Some(i) = inner.find(&info.app_name) {
                    inner.jobs[i].downloaded = p.downloaded;
                    if let Some(s) = speed {
                        inner.jobs[i].bytes_per_sec = s;
                    }
                }
            }
            if speed.is_some() {
                self.emit(&app);
            }
        };
        let result = run_job(&self.client, &self.store, &info, &manifest, old.as_deref(), stop, on_progress, RunOptions { limiter: Some(self.limiter.clone()), ..RunOptions::default() }).await;
        self.finish(&app, info, &manifest, result).await;
        self.pump(&app);
    }

    async fn finish(&self, app: &AppHandle, info: JobInfo, manifest: &Manifest, result: Result<(), InstallError>) {
        let cancel = {
            let inner = self.lock();
            inner.find(&info.app_name).is_some_and(|i| inner.jobs[i].cancel_requested)
        };
        if cancel {
            // Stay in the list (hidden, busy) until the files are gone, so Install can't race it.
            {
                let mut inner = self.lock();
                if let Some(i) = inner.find(&info.app_name) {
                    inner.jobs[i].state = JobState::Removing;
                }
            }
            self.emit(app);
            let app_name = info.app_name.clone();
            let result = self.discard(info).await;
            self.lock().end_removal(&app_name, result);
            self.emit(app);
            return;
        }
        let outcome = match result {
            Ok(()) => {
                let store = self.store.clone();
                let (i2, m2) = (info.clone(), manifest.clone());
                match tauri::async_runtime::spawn_blocking(move || store.finish_install(&i2, &m2)).await {
                    Ok(Ok(_)) => Ok(()),
                    Ok(Err(e)) => Err(InstallError::Io { folder: info.install_path.display().to_string(), detail: e.to_string() }),
                    Err(e) => Err(InstallError::Io { folder: info.install_path.display().to_string(), detail: e.to_string() }),
                }
            }
            Err(e) => Err(e),
        };
        let mut inner = self.lock();
        let Some(i) = inner.find(&info.app_name) else { return };
        match outcome {
            Ok(()) => {
                tracing::info!("finished {} ({} bytes downloaded)", info.app_name, info.download_size);
                inner.jobs.remove(i);
                drop(inner);
                let _ = app.emit("installed-changed", ());
                let (handle, name) = (app.clone(), info.app_name.clone());
                tauri::async_runtime::spawn(async move { crate::epic_sync::sync_one(&handle, &name).await });
            }
            Err(InstallError::Stopped) => {
                tracing::info!("paused {}", info.app_name);
                inner.jobs[i].state = JobState::Paused;
                inner.jobs[i].stop = None;
            }
            Err(e) => {
                let m = e.user_message();
                tracing::warn!("download of {} failed: {m}", info.app_name);
                inner.jobs[i].state = JobState::Error;
                inner.jobs[i].error = Some(m);
                inner.jobs[i].stop = None;
            }
        }
    }
}

/// Rough "already downloaded" for the bar and the space others reserve, until the job runs and
/// reports for real. Names only: checking every file of every paused download slowed startup.
fn already_downloaded(store: &InstallStore, info: &JobInfo, manifest: &Manifest, old: Option<&Manifest>) -> u64 {
    let finished = store.finished_names(info);
    if finished.is_empty() {
        return 0;
    }
    let left = fishy_core::install::plan::build_update_plan(manifest, old, &finished).map(|p| p.download_size).unwrap_or(info.download_size);
    info.download_size.saturating_sub(left)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fishy_core::install::manifest::ManifestMeta;

    fn job(app: &str, state: JobState) -> Job {
        let info = JobInfo {
            app_name: app.into(),
            title: format!("Title {app}"),
            namespace: "ns".into(),
            catalog_item_id: "c".into(),
            install_path: "C:\\Games\\x".into(),
            download_size: 1000,
            install_size: 2000,
            base_urls: vec![],
            update_from: None,
            imported: false,
        };
        Job::new(info, Arc::new(Manifest::new(ManifestMeta::default(), vec![], vec![])), state)
    }

    #[test]
    fn space_left_to_write_skips_the_asking_game() {
        let mut half = job("a", JobState::Paused);
        half.downloaded = 500; // half of 1000: an install still needs half of its 2000
        let mut upd = job("b", JobState::Queued);
        upd.info.update_from = Some("1".into());
        upd.downloaded = 200; // an update needs what's left to download: 800
        // A failed download (often: the disk was full) doesn't hold space for others.
        let inner = Inner { jobs: vec![half, upd, job("c", JobState::Paused), job("d", JobState::Error)], wiping: false };
        let left: Vec<u64> = inner.left_to_write("c").into_iter().map(|(_, b)| b).collect();
        assert_eq!(left, vec![1000, 800]);
    }

    #[test]
    fn a_queued_repair_counts_files_already_there_as_downloaded() {
        use fishy_core::install::manifest::testbuild::sample;
        let dir = tempfile::tempdir().unwrap();
        let store = InstallStore::at(dir.path().to_path_buf());
        let mut info = job("Tidepool", JobState::Queued).info;
        info.download_size = 900; // both sample chunks
        info.imported = true;
        store.mark_finished("Tidepool", "x", "Binaries/Tidepool.exe").unwrap();
        store.mark_finished("Tidepool", "x", "empty.cfg").unwrap();
        // readme.txt still needs the 600-byte chunk, so 300 of 900 count as done.
        assert_eq!(already_downloaded(&store, &info, &sample(), None), 300);
        assert_eq!(already_downloaded(&InstallStore::at(dir.path().join("none")), &info, &sample(), None), 0);
    }

    #[test]
    fn busy_while_a_job_is_downloading_or_being_removed() {
        assert!(!Inner { jobs: vec![job("p", JobState::Paused), job("q", JobState::Queued)], wiping: false }.busy());
        assert!(Inner { jobs: vec![job("d", JobState::Downloading)], wiping: false }.busy());
        assert!(Inner { jobs: vec![job("r", JobState::Removing)], wiping: false }.busy());
    }

    #[test]
    fn nothing_starts_while_wiping() {
        let mut inner = Inner { jobs: vec![job("q", JobState::Queued)], wiping: false };
        inner.wiping = true;
        assert_eq!(inner.next_to_start(), None);
        inner.wiping = false;
        assert_eq!(inner.next_to_start(), Some(0));
    }

    #[test]
    fn stop_all_cancels_the_running_job_and_pauses_the_queue() {
        let mut running = job("d", JobState::Downloading);
        let token = CancellationToken::new();
        running.stop = Some(token.clone());
        let mut inner = Inner { jobs: vec![running, job("q", JobState::Queued)], wiping: false };
        inner.stop_all();
        assert!(token.is_cancelled());
        assert_eq!(inner.jobs[1].state, JobState::Paused);
    }

    fn downloads_with(jobs: Vec<Job>) -> (tempfile::TempDir, Downloads) {
        let dir = tempfile::tempdir().unwrap();
        let d = Downloads::new(InstallStore::at(dir.path().to_path_buf()), InstallClient::new());
        d.lock().jobs = jobs;
        (dir, d)
    }

    #[test]
    fn a_download_that_wont_stop_refuses_the_wipe_and_keeps_everything() {
        let (_dir, d) = downloads_with(vec![job("stuck", JobState::Downloading), job("p", JobState::Paused)]);
        let result = tauri::async_runtime::block_on(d.stop_for_wipe(Duration::from_millis(200)));
        assert!(result.is_err());
        assert_eq!(d.lock().jobs.len(), 2, "nothing taken off the queue");
        assert!(!d.lock().wiping, "downloads can start again");
    }

    #[test]
    fn stopped_downloads_are_taken_and_new_ones_refused_until_the_wipe_ends() {
        let (_dir, d) = downloads_with(vec![job("p", JobState::Paused), job("q", JobState::Queued)]);
        let taken = tauri::async_runtime::block_on(d.stop_for_wipe(Duration::from_millis(200))).unwrap();
        assert_eq!(taken.iter().map(|j| j.app_name.as_str()).collect::<Vec<_>>(), vec!["p", "q"]);
        assert!(d.lock().jobs.is_empty());
        assert!(d.lock().wiping);
        d.end_wipe();
        assert!(!d.lock().wiping);
    }

    #[test]
    fn take_all_empties_the_queue() {
        let mut inner = Inner { jobs: vec![job("p", JobState::Paused), job("e", JobState::Error)], wiping: false };
        let taken: Vec<String> = inner.take_all().into_iter().map(|j| j.app_name).collect();
        assert_eq!(taken, vec!["p", "e"]);
        assert!(inner.jobs.is_empty());
    }

    #[test]
    fn view_orders_running_then_queue_then_paused() {
        let inner = Inner {
            jobs: vec![job("p", JobState::Paused), job("q1", JobState::Queued), job("d", JobState::Downloading), job("e", JobState::Error), job("q2", JobState::Queued)],
            wiping: false,
        };
        let v = inner.view();
        let order: Vec<(&str, &str)> = v.items.iter().map(|i| (i.app_name.as_str(), i.state)).collect();
        assert_eq!(order, vec![("d", "downloading"), ("q1", "queued"), ("q2", "queued"), ("p", "paused"), ("e", "error")]);
        assert_eq!(v.items[0].total, 1000);
        assert_eq!(v.items[0].title, "Title d");
    }

    #[test]
    fn a_job_being_removed_is_hidden_but_still_blocks_a_reinstall() {
        let mut inner = Inner { jobs: vec![job("q", JobState::Queued), job("r", JobState::Queued)], wiping: false };
        let info = inner.begin_removal("q").expect("not running, so the caller deletes the files");
        assert_eq!(info.app_name, "q");
        assert_eq!(inner.jobs[0].state, JobState::Removing);
        assert!(inner.find("q").is_some(), "still busy: Install must not start it again yet");
        assert_eq!(inner.view().items.iter().map(|i| i.app_name.as_str()).collect::<Vec<_>>(), vec!["r"]);
        assert_eq!(inner.next_to_start(), Some(1));
        inner.end_removal("q", Ok(()));
        assert!(inner.find("q").is_none());
    }

    #[test]
    fn removing_a_running_job_asks_it_to_stop_first() {
        let mut inner = Inner { jobs: vec![job("d", JobState::Downloading)], wiping: false };
        let stop = CancellationToken::new();
        inner.jobs[0].stop = Some(stop.clone());
        assert!(inner.begin_removal("d").is_none(), "the running task deletes once it has stopped");
        assert!(stop.is_cancelled());
        assert!(inner.jobs[0].cancel_requested);
    }

    #[test]
    fn a_failed_removal_comes_back_with_the_reason() {
        let mut inner = Inner { jobs: vec![job("q", JobState::Queued)], wiping: false };
        inner.begin_removal("q");
        inner.end_removal("q", Err("Couldn't delete some files. Close the game and try again.".into()));
        assert_eq!(inner.jobs[0].state, JobState::Error);
        let v = inner.view();
        assert_eq!(v.items[0].error.as_deref(), Some("Couldn't delete some files. Close the game and try again."));
    }

    fn update(app: &str, state: JobState) -> Job {
        let mut j = job(app, state);
        j.info.update_from = Some("1.0".into());
        j
    }

    #[test]
    fn view_marks_update_jobs() {
        let inner = Inner { jobs: vec![update("u", JobState::Paused), job("i", JobState::Queued)], wiping: false };
        let v = inner.view();
        assert_eq!(v.items.iter().map(|i| (i.app_name.as_str(), i.update)).collect::<Vec<_>>(), vec![("i", false), ("u", true)]);
    }

    #[test]
    fn cancelling_an_update_is_refused() {
        let mut inner = Inner { jobs: vec![update("u", JobState::Downloading)], wiping: false };
        let stop = CancellationToken::new();
        inner.jobs[0].stop = Some(stop.clone());
        assert!(inner.is_update("u"));
        assert!(inner.begin_removal("u").is_none());
        assert!(!stop.is_cancelled() && !inner.jobs[0].cancel_requested);
        assert_eq!(inner.jobs[0].state, JobState::Downloading);
    }

    #[test]
    fn uninstall_takes_only_a_stopped_update() {
        let mut inner = Inner { jobs: vec![update("run", JobState::Downloading), update("p", JobState::Paused), job("i", JobState::Paused)], wiping: false };
        assert!(!inner.take_stopped_update("run"));
        assert!(!inner.take_stopped_update("i"), "a normal install isn't an update");
        assert!(inner.take_stopped_update("p"));
        assert!(inner.find("p").is_none());
        assert!(inner.take_stopped_update("none"), "nothing to take is fine");
    }

    #[test]
    fn restored_update_job_keeps_its_old_manifest() {
        use fishy_core::install::manifest::testbuild::{build_binary, sample};
        let dir = tempfile::tempdir().unwrap();
        let store = InstallStore::at(dir.path().join("data"));
        let mut info = job("Tide", JobState::Paused).info;
        info.install_path = dir.path().join("game");
        store.save_job(&info, &build_binary(&sample(), true)).unwrap();
        store.finish_install(&info, &sample()).unwrap(); // the installed version
        info.update_from = Some("1.0.0-CL1".into());
        store.save_job(&info, &build_binary(&sample(), true)).unwrap(); // a paused update
        let d = Downloads::new(store, InstallClient::new());
        let inner = d.lock();
        assert!(inner.jobs[0].old.is_some());
        assert!(inner.view().items[0].update);
    }

    #[test]
    fn next_to_start_waits_for_the_running_job() {
        let mut inner = Inner { jobs: vec![job("p", JobState::Paused), job("q1", JobState::Queued), job("q2", JobState::Queued)], wiping: false };
        assert_eq!(inner.next_to_start(), Some(1));
        inner.jobs[1].state = JobState::Downloading;
        assert_eq!(inner.next_to_start(), None);
        inner.jobs.remove(1);
        assert_eq!(inner.next_to_start(), Some(1));
        let empty = Inner { jobs: vec![job("p", JobState::Paused)], wiping: false };
        assert_eq!(empty.next_to_start(), None);
    }
}
