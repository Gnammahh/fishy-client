//! The Downloads manager: one game downloads at a time, the rest wait in a queue (spec §6c).
//! It owns a stop token per running job and tells the UI about changes with `downloads-changed`.
use fishy_core::install::engine::{run_download, Progress};
use fishy_core::install::manifest::Manifest;
use fishy_core::install::speed::SpeedMeter;
use fishy_core::install::store::{InstallStore, JobInfo};
use fishy_core::install::uninstall::discard_download;
use fishy_core::install::client::InstallClient;
use fishy_core::install::InstallError;
use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

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
    pub state: JobState,
    pub downloaded: u64,
    pub bytes_per_sec: f64,
    pub error: Option<String>,
    stop: Option<CancellationToken>,
    cancel_requested: bool,
}

impl Job {
    pub fn new(info: JobInfo, manifest: Arc<Manifest>, state: JobState) -> Self {
        Self { info, manifest, state, downloaded: 0, bytes_per_sec: 0.0, error: None, stop: None, cancel_requested: false }
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
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DownloadsView {
    pub items: Vec<DownloadItem>,
}

#[derive(Default)]
pub struct Inner {
    pub jobs: Vec<Job>,
}

impl Inner {
    /// Running job first, then the queue in order, then paused and failed ones.
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
                })
                .collect(),
        }
    }

    /// The job to start now: the first queued one, if nothing is running.
    pub fn next_to_start(&self) -> Option<usize> {
        if self.jobs.iter().any(|j| j.state == JobState::Downloading) {
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
                let manifest = fishy_core::install::manifest::parse_manifest(&bytes).ok()?;
                let mut job = Job::new(info, Arc::new(manifest), JobState::Paused);
                let finished = store.finished_files(&job.info);
                // Rough "already downloaded" for the bar, until the job runs and reports for real.
                let done = fishy_core::install::plan::build_plan(&job.manifest, &finished).map(|p| p.download_size).unwrap_or(0);
                job.downloaded = job.info.download_size.saturating_sub(done);
                Some(job)
            })
            .collect();
        Self { store: Arc::new(store), client, inner: Arc::new(Mutex::new(Inner { jobs })) }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn view(&self) -> DownloadsView {
        self.lock().view()
    }

    pub fn has_job(&self, app: &str) -> bool {
        self.lock().find(app).is_some()
    }

    fn emit(&self, app: &AppHandle) {
        let _ = app.emit("downloads-changed", self.view());
    }

    /// Adds a new download to the queue (or starts it).
    pub fn enqueue(&self, app: &AppHandle, info: JobInfo, manifest: Arc<Manifest>) {
        {
            let mut inner = self.lock();
            if inner.find(&info.app_name).is_some() {
                return;
            }
            inner.jobs.push(Job::new(info, manifest, JobState::Queued));
        }
        self.pump(app);
    }

    pub fn pause(&self, app: &AppHandle, app_name: &str) {
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
        // A running job's own task deletes the files once it has stopped writing.
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

    /// Log out: the running download pauses and nothing new starts.
    pub fn pause_all(&self, app: &AppHandle) {
        {
            let mut inner = self.lock();
            for job in &mut inner.jobs {
                match job.state {
                    JobState::Downloading => job.stop.iter().for_each(CancellationToken::cancel),
                    JobState::Queued => job.state = JobState::Paused,
                    _ => {}
                }
            }
        }
        self.emit(app);
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
                (job.info.clone(), job.manifest.clone(), stop)
            })
        };
        self.emit(app);
        if let Some((info, manifest, stop)) = started {
            let this = self.clone();
            let app = app.clone();
            tauri::async_runtime::spawn(async move { this.run(app, info, manifest, stop).await });
        }
    }

    async fn run(self, app: AppHandle, info: JobInfo, manifest: Arc<Manifest>, stop: CancellationToken) {
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
        let result = run_download(&self.client, &self.store, &info, &manifest, stop, on_progress).await;
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
                inner.jobs.remove(i);
                drop(inner);
                let _ = app.emit("installed-changed", ());
            }
            Err(InstallError::Stopped) => {
                inner.jobs[i].state = JobState::Paused;
                inner.jobs[i].stop = None;
            }
            Err(e) => {
                inner.jobs[i].state = JobState::Error;
                inner.jobs[i].error = Some(e.user_message());
                inner.jobs[i].stop = None;
            }
        }
    }
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
        };
        Job::new(info, Arc::new(Manifest::new(ManifestMeta::default(), vec![], vec![])), state)
    }

    #[test]
    fn view_orders_running_then_queue_then_paused() {
        let inner = Inner {
            jobs: vec![job("p", JobState::Paused), job("q1", JobState::Queued), job("d", JobState::Downloading), job("e", JobState::Error), job("q2", JobState::Queued)],
        };
        let v = inner.view();
        let order: Vec<(&str, &str)> = v.items.iter().map(|i| (i.app_name.as_str(), i.state)).collect();
        assert_eq!(order, vec![("d", "downloading"), ("q1", "queued"), ("q2", "queued"), ("p", "paused"), ("e", "error")]);
        assert_eq!(v.items[0].total, 1000);
        assert_eq!(v.items[0].title, "Title d");
    }

    #[test]
    fn a_job_being_removed_is_hidden_but_still_blocks_a_reinstall() {
        let mut inner = Inner { jobs: vec![job("q", JobState::Queued), job("r", JobState::Queued)] };
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
        let mut inner = Inner { jobs: vec![job("d", JobState::Downloading)] };
        let stop = CancellationToken::new();
        inner.jobs[0].stop = Some(stop.clone());
        assert!(inner.begin_removal("d").is_none(), "the running task deletes once it has stopped");
        assert!(stop.is_cancelled());
        assert!(inner.jobs[0].cancel_requested);
    }

    #[test]
    fn a_failed_removal_comes_back_with_the_reason() {
        let mut inner = Inner { jobs: vec![job("q", JobState::Queued)] };
        inner.begin_removal("q");
        inner.end_removal("q", Err("Couldn't delete some files. Close the game and try again.".into()));
        assert_eq!(inner.jobs[0].state, JobState::Error);
        let v = inner.view();
        assert_eq!(v.items[0].error.as_deref(), Some("Couldn't delete some files. Close the game and try again."));
    }

    #[test]
    fn next_to_start_waits_for_the_running_job() {
        let mut inner = Inner { jobs: vec![job("p", JobState::Paused), job("q1", JobState::Queued), job("q2", JobState::Queued)] };
        assert_eq!(inner.next_to_start(), Some(1));
        inner.jobs[1].state = JobState::Downloading;
        assert_eq!(inner.next_to_start(), None);
        inner.jobs.remove(1);
        assert_eq!(inner.next_to_start(), Some(1));
        let empty = Inner { jobs: vec![job("p", JobState::Paused)] };
        assert_eq!(empty.next_to_start(), None);
    }
}
