//! The Downloads manager: one game downloads at a time, the rest wait in a queue (spec §6c).
//! It owns a stop token per running job and tells the UI about changes with `downloads-changed`.
use fishy_core::install::engine::{run_job, Progress, RunOptions};
use fishy_core::install::manifest::{parse_manifest, Manifest};
use fishy_core::install::parts::{keep_siblings_files, only_parts, sibling_files};
use fishy_core::install::speed::SpeedMeter;
use fishy_core::install::store::{InstallStore, JobInfo};
use fishy_core::install::pieces::{remove_piece, Removed};
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
/// Removing a piece while its game downloads (spec §6u).
pub const PIECE_BUSY: &str = "Pause the game's downloads first, then remove it.";

/// An update begun in place before 1.10.0 that already changed files (spec §6u).
pub const UPDATE_IN_PLACE: &str = "This update already changed some files, so it can't be cancelled. Pause it or let it finish.";

/// After a run ends with Cancel pressed: whether what it wrote is thrown away. An update that
/// already moved its files into the game is finished instead (spec §6u).
pub fn discard_after_run(cancel_requested: bool, is_update: bool, ok: bool) -> bool {
    cancel_requested && !(is_update && ok)
}

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
    /// The game's Priority download switch (spec §6t).
    priority: bool,
    /// Stopped by Start now → Pause: it waits again instead of pausing.
    requeue: bool,
}

impl Job {
    pub fn new(info: JobInfo, manifest: Arc<Manifest>, state: JobState) -> Self {
        Self { info, manifest, old: None, state, downloaded: 0, bytes_per_sec: 0.0, error: None, stop: None, cancel_requested: false, priority: false, requeue: false }
    }
}

/// A job just marked as downloading: what its task needs.
pub struct Started {
    pub info: JobInfo,
    pub manifest: Arc<Manifest>,
    pub old: Option<Arc<Manifest>>,
    pub stop: CancellationToken,
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

/// Start now on a waiting game (spec §6t): pause the running one, or download both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartNow {
    Pause,
    Both,
}

const NOT_WAITING: &str = "That game isn't waiting.";
const TWO_RUNNING: &str = "Two games are already downloading.";

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

    /// The jobs that are downloading or waiting: what a pause for an app update has to resume
    /// if the update fails (spec §6j).
    pub fn active_names(&self) -> Vec<String> {
        self.jobs.iter().filter(|j| matches!(j.state, JobState::Downloading | JobState::Queued)).map(|j| j.info.app_name.clone()).collect()
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
            job.requeue = false;
        }
    }

    /// A waiting priority job moves in front of the waiting non-priority jobs ahead of it.
    pub fn prioritize(&mut self, app: &str) {
        let Some(i) = self.find(app) else { return };
        if !self.jobs[i].priority || self.jobs[i].state != JobState::Queued {
            return;
        }
        if let Some(to) = self.jobs[..i].iter().position(|j| j.state == JobState::Queued && !j.priority) {
            let job = self.jobs.remove(i);
            self.jobs.insert(to, job);
        }
    }

    /// The Priority download switch changed; on moves a waiting job forward, off doesn't move it.
    pub fn set_priority(&mut self, app: &str, on: bool) {
        let Some(i) = self.find(app) else { return };
        self.jobs[i].priority = on;
        self.prioritize(app);
    }

    /// Start now (spec §6t). Ok(None): it's first in the queue, which starts it when nothing is
    /// downloading (with Pause, the running one was asked to stop and will wait behind it).
    /// Ok(Some(i)): start job i now, next to the running one.
    pub fn start_now(&mut self, app: &str, mode: StartNow) -> Result<Option<usize>, &'static str> {
        let i = self.find(app).filter(|&i| self.jobs[i].state == JobState::Queued && !self.wiping).ok_or(NOT_WAITING)?;
        let running: Vec<usize> = (0..self.jobs.len()).filter(|&k| self.jobs[k].state == JobState::Downloading).collect();
        match (running.len(), mode) {
            (0, _) => {}
            (1, StartNow::Both) => return Ok(Some(i)),
            (1, StartNow::Pause) => {
                let r = &mut self.jobs[running[0]];
                r.requeue = true;
                r.stop.iter().for_each(CancellationToken::cancel);
            }
            _ => return Err(TWO_RUNNING),
        }
        let job = self.jobs.remove(i);
        self.jobs.insert(0, job);
        Ok(None)
    }

    /// The user's Pause: a running job is asked to stop and then pauses (even after Start now →
    /// Pause asked it to wait again); a waiting one pauses at once.
    pub fn pause_job(&mut self, app: &str) {
        let Some(i) = self.find(app) else { return };
        let job = &mut self.jobs[i];
        job.requeue = false;
        match job.state {
            // The task notices, stops and marks it Paused.
            JobState::Downloading => job.stop.iter().for_each(CancellationToken::cancel),
            JobState::Queued => job.state = JobState::Paused,
            _ => {}
        }
    }

    /// A running job failed: it stops with the reason (and doesn't wait again).
    pub fn failed(&mut self, app: &str, message: String) {
        let Some(i) = self.find(app) else { return };
        let job = &mut self.jobs[i];
        job.state = JobState::Error;
        job.error = Some(message);
        job.stop = None;
        job.requeue = false;
    }

    /// A running job stopped writing without an error: Paused, or (after Start now → Pause)
    /// waiting again right behind the first waiting job.
    pub fn stopped(&mut self, app: &str) {
        let Some(i) = self.find(app) else { return };
        let job = &mut self.jobs[i];
        job.stop = None;
        if !std::mem::take(&mut job.requeue) {
            job.state = JobState::Paused;
            return;
        }
        let mut job = self.jobs.remove(i);
        job.state = JobState::Queued;
        let to = self.jobs.iter().position(|j| j.state == JobState::Queued).map_or(self.jobs.len(), |q| q + 1);
        self.jobs.insert(to, job);
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

    /// Marks this waiting game's job as downloading and hands back what its task needs. By name,
    /// under the caller's one lock, so a list changed in between can't start the wrong job.
    pub fn begin(&mut self, app: &str) -> Option<Started> {
        let i = self.find(app).filter(|&i| self.jobs[i].state == JobState::Queued)?;
        let job = &mut self.jobs[i];
        let stop = CancellationToken::new();
        job.state = JobState::Downloading;
        job.stop = Some(stop.clone());
        job.cancel_requested = false;
        job.requeue = false;
        Some(Started { info: job.info.clone(), manifest: job.manifest.clone(), old: job.old.clone(), stop })
    }

    /// Starts the first waiting job, if nothing is running.
    pub fn begin_next(&mut self) -> Option<Started> {
        let name = self.jobs[self.next_to_start()?].info.app_name.clone();
        self.begin(&name)
    }

    /// Start now, and start whatever should start now, in one go.
    pub fn start_now_begin(&mut self, app: &str, mode: StartNow) -> Result<Option<Started>, &'static str> {
        Ok(match self.start_now(app, mode)? {
            Some(_) => self.begin(app),
            None => self.begin_next(),
        })
    }

    /// Why this game can't be uninstalled because of its download, if it can't. A paused or
    /// failed update goes with the game (`take_stopped_update`).
    pub fn uninstall_refusal(&self, app: &str) -> Option<&'static str> {
        let job = &self.jobs[self.find(app)?];
        if job.info.update_from.is_none() {
            Some("Finish or cancel its download first, then uninstall.")
        } else if matches!(job.state, JobState::Paused | JobState::Error) {
            None
        } else {
            Some("Pause the update first, then uninstall.")
        }
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
    /// Told the game's name when its download, update or repair finishes (Verify forgets its
    /// old result then).
    on_finished: Arc<Mutex<Vec<FinishedHook>>>,
}

type FinishedHook = Box<dyn Fn(&str) + Send + Sync>;

impl Downloads {
    /// Downloads left from last time come back as Paused.
    pub fn new(store: InstallStore, client: InstallClient) -> Self {
        let jobs = store.pending_jobs().into_iter().filter_map(|info| load_job(&store, info)).collect();
        Self { store: Arc::new(store), client, limiter: Arc::new(SpeedLimiter::new()), inner: Arc::new(Mutex::new(Inner { jobs, wiping: false })), on_finished: Arc::default() }
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
            job.priority = self.priority_of(&job.info.app_name);
            let name = job.info.app_name.clone();
            inner.jobs.push(job);
            inner.prioritize(&name);
        }
        self.pump(app);
    }

    /// The game's Priority download switch, from its installed record (spec §6t).
    fn priority_of(&self, app_name: &str) -> bool {
        self.store.installed().iter().any(|g| g.app_name == app_name && g.options.priority)
    }

    /// The Priority download switch changed in the Manage box.
    pub fn set_priority(&self, app: &AppHandle, app_name: &str, on: bool) {
        self.lock().set_priority(app_name, on);
        self.emit(app);
    }

    /// Start now on a waiting game (spec §6t).
    pub fn start_now(&self, app: &AppHandle, app_name: &str, mode: StartNow) -> Result<DownloadsView, String> {
        let started = self.lock().start_now_begin(app_name, mode).map_err(str::to_string)?;
        tracing::info!("start now: {}", if mode == StartNow::Pause { "pause" } else { "both" });
        self.launch(app, started);
        Ok(self.view())
    }

    /// Calls `f` with the game's name whenever a download, update or repair finishes.
    pub fn when_finished(&self, f: impl Fn(&str) + Send + Sync + 'static) {
        self.on_finished.lock().unwrap_or_else(|p| p.into_inner()).push(Box::new(f));
    }

    /// The game's files are now complete: its job leaves the list and the hooks are told.
    fn finished_ok(&self, app_name: &str) {
        {
            let mut inner = self.lock();
            if let Some(i) = inner.find(app_name) {
                inner.jobs.remove(i);
            }
        }
        for f in self.on_finished.lock().unwrap_or_else(|p| p.into_inner()).iter() {
            f(app_name);
        }
    }

    pub fn pause(&self, app: &AppHandle, app_name: &str) {
        tracing::info!("pause {app_name}");
        self.lock().pause_job(app_name);
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
            inner.prioritize(app_name);
        }
        self.pump(app);
    }

    /// Stops the download and deletes what it wrote.
    /// Removes an add-on piece (spec §6u), under the queue's lock so nothing of its game starts
    /// meanwhile: its waiting update leaves the list and the game's is reloaded, narrowed.
    pub fn remove_piece(&self, app_name: &str) -> Result<Removed, String> {
        let mut inner = self.lock();
        let game = self.store.installed().into_iter().find(|g| g.app_name == app_name).and_then(|g| g.parent).ok_or_else(|| InstallError::NotInstalled.user_message())?;
        let of_game = |j: &Job| j.info.app_name == game || j.info.app_name == app_name || j.info.parent.as_deref() == Some(game.as_str());
        if inner.wiping || inner.jobs.iter().any(|j| of_game(j) && matches!(j.state, JobState::Downloading | JobState::Removing)) {
            return Err(PIECE_BUSY.to_string());
        }
        let removed = remove_piece(&self.store, app_name).map_err(|e| e.user_message())?;
        inner.jobs.retain(|j| j.info.app_name != app_name);
        if let Some(i) = inner.find(&game) {
            let saved = self.store.pending_jobs().into_iter().find(|j| j.app_name == game);
            if let Some(mut job) = saved.and_then(|info| load_job(&self.store, info)) {
                let before = &inner.jobs[i];
                job.state = before.state;
                job.error = before.error.clone();
                job.priority = before.priority;
                inner.jobs[i] = job;
            }
        }
        Ok(removed)
    }

    /// Why Cancel can't stop this game's download, if it can't (spec §6u).
    pub fn cancel_refusal(&self, app_name: &str) -> Option<&'static str> {
        let info = {
            let inner = self.lock();
            inner.jobs[inner.find(app_name)?].info.clone()
        };
        (!fishy_core::install::uninstall::update_can_be_cancelled(&self.store, &info)).then_some(UPDATE_IN_PLACE)
    }

    pub async fn cancel(&self, app: &AppHandle, app_name: &str) -> Result<(), String> {
        if let Some(refusal) = self.cancel_refusal(app_name) {
            return Err(refusal.to_string());
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
        tauri::async_runtime::spawn_blocking(move || {
            if info.update_from.is_some() {
                tracing::info!("update: cancelled");
                fishy_core::install::uninstall::discard_update(&store, &info)
            } else {
                discard_download(&store, &info)
            }
        })
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.user_message())
    }

    /// For Uninstall: drops a paused or failed update of this game. False if one is running or queued.
    pub fn take_stopped_update(&self, app_name: &str) -> bool {
        self.lock().take_stopped_update(app_name)
    }

    /// Why this game can't be uninstalled because of its download, if it can't.
    pub fn uninstall_refusal(&self, app_name: &str) -> Option<&'static str> {
        self.lock().uninstall_refusal(app_name)
    }

    pub fn active_names(&self) -> Vec<String> {
        self.lock().active_names()
    }

    /// Delete all my data is running: nothing may be queued.
    pub fn is_wiping(&self) -> bool {
        self.lock().wiping
    }

    /// Wait (up to `timeout`) until nothing is writing, e.g. after `pause_all`.
    pub async fn wait_idle(&self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        while self.lock().busy() && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
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

    /// Starts the next queued job if nothing is running (chosen and started under one lock).
    fn pump(&self, app: &AppHandle) {
        let started = self.lock().begin_next();
        self.launch(app, started);
    }

    /// Runs the task of a job just started (if any) and reports the change.
    fn launch(&self, app: &AppHandle, started: Option<Started>) {
        self.emit(app);
        if let Some(Started { info, manifest, old, stop }) = started {
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
            let asked = inner.find(&info.app_name).is_some_and(|i| inner.jobs[i].cancel_requested);
            discard_after_run(asked, info.update_from.is_some(), result.is_ok())
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
        if inner.find(&info.app_name).is_none() {
            return;
        }
        match outcome {
            Ok(()) => {
                tracing::info!("finished {} ({} bytes downloaded)", info.app_name, info.download_size);
                drop(inner);
                self.finished_ok(&info.app_name);
                let _ = app.emit("download-finished", &info.app_name);
                let _ = app.emit("installed-changed", ());
            }
            Err(InstallError::Stopped) => {
                tracing::info!("paused {}", info.app_name);
                inner.stopped(&info.app_name);
            }
            Err(e) => {
                let m = e.user_message();
                tracing::warn!("download of {} failed: {m}", info.app_name);
                inner.failed(&info.app_name, m);
            }
        }
    }
}

/// A saved download as a paused job, with its manifest filtered to the chosen parts.
fn load_job(store: &InstallStore, info: JobInfo) -> Option<Job> {
    let bytes = store.job_manifest(&info.app_name).ok()?;
    // Only the files of the chosen parts (spec §6n); None keeps every file.
    let manifest = only_parts(parse_manifest(&bytes).ok()?, info.parts.as_deref());
    let mut job = Job::new(info, Arc::new(manifest), JobState::Paused);
    if job.info.update_from.is_some() {
        // Without the old manifest the update still works, it just downloads everything.
        let old_parts = store.installed().into_iter().find(|g| g.app_name == job.info.app_name).and_then(|g| g.parts);
        let siblings = sibling_files(store, &job.info.app_name);
        job.old = store
            .installed_manifest(&job.info.app_name)
            .ok()
            .and_then(|b| parse_manifest(&b).ok())
            .map(|m| Arc::new(keep_siblings_files(only_parts(m, old_parts.as_deref()), &job.manifest, &siblings)));
    }
    job.downloaded = already_downloaded(store, &job.info, &job.manifest, job.old.as_deref());
    // The Priority download switch is kept through a restart (spec §6t).
    job.priority = store.installed().iter().any(|g| g.app_name == job.info.app_name && g.options.priority);
    Some(job)
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
            parts: None,
            parent: None,
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
    fn active_names_are_the_running_and_queued_jobs() {
        let inner = Inner {
            jobs: vec![job("run", JobState::Downloading), job("wait", JobState::Queued), job("p", JobState::Paused), job("e", JobState::Error)],
            wiping: false,
        };
        assert_eq!(inner.active_names(), vec!["run".to_string(), "wait".to_string()]);
    }

    #[test]
    fn busy_while_a_job_is_downloading_or_being_removed() {
        assert!(!Inner { jobs: vec![job("p", JobState::Paused), job("q", JobState::Queued)], wiping: false }.busy());
        assert!(Inner { jobs: vec![job("d", JobState::Downloading)], wiping: false }.busy());
        assert!(Inner { jobs: vec![job("r", JobState::Removing)], wiping: false }.busy());
    }

    fn prio(app: &str, state: JobState) -> Job {
        let mut j = job(app, state);
        j.priority = true;
        j
    }

    fn queued_order(inner: &Inner) -> Vec<String> {
        inner.jobs.iter().filter(|j| j.state == JobState::Queued).map(|j| j.info.app_name.clone()).collect()
    }

    #[test]
    fn priority_job_goes_before_waiting_non_priority_ones() {
        let mut inner = Inner { jobs: vec![job("run", JobState::Downloading)], wiping: false };
        for j in [job("a", JobState::Queued), prio("b", JobState::Queued), prio("c", JobState::Queued)] {
            let name = j.info.app_name.clone();
            inner.jobs.push(j);
            inner.prioritize(&name);
        }
        assert_eq!(queued_order(&inner), vec!["b", "c", "a"]);
        assert_eq!(inner.jobs[0].info.app_name, "run");
    }

    #[test]
    fn turning_priority_on_moves_a_waiting_job_forward() {
        let mut inner = Inner { jobs: vec![job("a", JobState::Queued), job("b", JobState::Queued), job("c", JobState::Queued)], wiping: false };
        inner.set_priority("c", true);
        assert_eq!(queued_order(&inner), vec!["c", "a", "b"]);
        inner.set_priority("c", false);
        assert_eq!(queued_order(&inner), vec!["c", "a", "b"], "turning it off doesn't move it");
        assert!(!inner.jobs[inner.find("c").unwrap()].priority);
    }

    #[test]
    fn a_paused_priority_job_isnt_moved() {
        let mut inner = Inner { jobs: vec![job("a", JobState::Queued), job("p", JobState::Paused)], wiping: false };
        inner.set_priority("p", true);
        assert_eq!(inner.jobs[1].info.app_name, "p");
    }

    #[test]
    fn start_now_with_nothing_running_returns_none_and_puts_it_first() {
        let mut inner = Inner { jobs: vec![job("a", JobState::Queued), job("b", JobState::Queued)], wiping: false };
        assert_eq!(inner.start_now("b", StartNow::Pause), Ok(None));
        assert_eq!(inner.next_to_start().map(|i| inner.jobs[i].info.app_name.clone()), Some("b".into()));
    }

    #[test]
    fn both_needs_exactly_one_running() {
        let mut inner = Inner { jobs: vec![job("run", JobState::Downloading), job("a", JobState::Queued)], wiping: false };
        assert_eq!(inner.start_now("a", StartNow::Both), Ok(Some(1)));
        let mut two = Inner { jobs: vec![job("r1", JobState::Downloading), job("r2", JobState::Downloading), job("a", JobState::Queued)], wiping: false };
        for mode in [StartNow::Both, StartNow::Pause] {
            assert_eq!(two.start_now("a", mode), Err("Two games are already downloading."));
        }
    }

    #[test]
    fn pause_mode_requeues_right_behind_the_chosen_job() {
        let mut inner = Inner { jobs: vec![job("run", JobState::Downloading), job("a", JobState::Queued), job("b", JobState::Queued)], wiping: false };
        assert_eq!(inner.start_now("b", StartNow::Pause), Ok(None));
        assert!(inner.jobs[inner.find("run").unwrap()].requeue);
        assert_eq!(inner.next_to_start(), None, "still writing until it stops");
        inner.stopped("run");
        assert_eq!(queued_order(&inner), vec!["b", "run", "a"]);
        assert_eq!(inner.next_to_start().map(|i| inner.jobs[i].info.app_name.clone()), Some("b".into()));
        assert!(!inner.jobs[inner.find("run").unwrap()].requeue);
    }

    #[test]
    fn a_plain_stop_still_pauses() {
        let mut inner = Inner { jobs: vec![job("run", JobState::Downloading)], wiping: false };
        inner.stopped("run");
        assert_eq!(inner.jobs[0].state, JobState::Paused);
    }

    #[test]
    fn start_now_refuses_a_job_being_cancelled() {
        let mut inner = Inner { jobs: vec![job("r", JobState::Removing), job("p", JobState::Paused), job("q", JobState::Queued)], wiping: false };
        assert_eq!(inner.start_now("r", StartNow::Pause), Err("That game isn't waiting."));
        assert_eq!(inner.start_now("p", StartNow::Both), Err("That game isn't waiting."));
        assert_eq!(inner.start_now("nope", StartNow::Both), Err("That game isn't waiting."));
        inner.wiping = true;
        assert_eq!(inner.start_now("q", StartNow::Pause), Err("That game isn't waiting."));
    }

    #[test]
    fn stop_all_clears_requeue() {
        let mut inner = Inner { jobs: vec![job("run", JobState::Downloading), job("a", JobState::Queued)], wiping: false };
        inner.start_now("a", StartNow::Pause).unwrap();
        inner.stop_all();
        inner.stopped("run");
        assert_eq!(inner.jobs[inner.find("run").unwrap()].state, JobState::Paused);
    }

    #[test]
    fn queue_never_starts_a_third() {
        let inner = Inner { jobs: vec![job("r1", JobState::Downloading), job("r2", JobState::Downloading), job("q", JobState::Queued)], wiping: false };
        assert_eq!(inner.next_to_start(), None);
    }

    #[test]
    fn pause_and_errors_clear_requeue() {
        let mut inner = Inner { jobs: vec![job("run", JobState::Downloading), job("a", JobState::Queued)], wiping: false };
        inner.start_now("a", StartNow::Pause).unwrap();
        inner.pause_job("run"); // the user's Pause wins over Start now's
        inner.stopped("run");
        assert_eq!(inner.jobs[inner.find("run").unwrap()].state, JobState::Paused);

        let mut inner = Inner { jobs: vec![job("run", JobState::Downloading), job("a", JobState::Queued)], wiping: false };
        inner.start_now("a", StartNow::Pause).unwrap();
        inner.failed("run", "boom".into());
        let i = inner.find("run").unwrap();
        assert_eq!((inner.jobs[i].state, inner.jobs[i].requeue), (JobState::Error, false));
        assert_eq!(inner.jobs[i].error.as_deref(), Some("boom"));
    }

    #[test]
    fn a_queued_pause_still_pauses() {
        let mut inner = Inner { jobs: vec![job("q", JobState::Queued)], wiping: false };
        inner.pause_job("q");
        assert_eq!(inner.jobs[0].state, JobState::Paused);
    }

    #[test]
    fn priority_survives_a_restart() {
        use fishy_core::install::manifest::testbuild::{build_binary, sample};
        let dir = tempfile::tempdir().unwrap();
        let store = InstallStore::at(dir.path().join("data"));
        let mut info = job("Tidepool", JobState::Paused).info;
        info.install_path = dir.path().join("game");
        let bytes = build_binary(&sample(), true);
        store.save_job(&info, &bytes).unwrap();
        store.finish_install(&info, &sample()).unwrap();
        store.set_options("Tidepool", None, true, "", false).unwrap();
        let update = JobInfo { update_from: Some("0.9".into()), ..info };
        store.save_job(&update, &bytes).unwrap();
        let d = Downloads::new(store, InstallClient::new());
        assert!(d.lock().jobs[0].priority);
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
    fn a_running_update_is_cancelled_like_an_install() {
        let mut inner = Inner { jobs: vec![update("u", JobState::Downloading), update("p", JobState::Paused)], wiping: false };
        let stop = CancellationToken::new();
        inner.jobs[0].stop = Some(stop.clone());
        assert!(inner.begin_removal("u").is_none(), "its task deletes what it built");
        assert!(stop.is_cancelled() && inner.jobs[0].cancel_requested);
        assert_eq!(inner.begin_removal("p").map(|j| j.app_name), Some("p".to_string()));
        assert_eq!(inner.jobs[1].state, JobState::Removing);
    }

    #[test]
    fn an_update_that_already_changed_files_refuses_cancel() {
        let mut u = update("u", JobState::Paused);
        let (dir, d) = downloads_with(vec![]);
        u.info.install_path = dir.path().join("game");
        d.lock().jobs.push(u);
        assert_eq!(d.cancel_refusal("u"), None, "nothing written yet");
        d.store.mark_finished("u", "x", "big.pak").unwrap(); // written in place before 1.10.0
        assert_eq!(d.cancel_refusal("u"), Some(UPDATE_IN_PLACE));
        assert_eq!(d.cancel_refusal("none"), None);
    }

    #[test]
    fn cancelling_a_paused_update_deletes_only_what_it_built_aside() {
        let mut u = update("u", JobState::Paused);
        let (dir, d) = downloads_with(vec![]);
        let game = dir.path().join("game");
        u.info.install_path = game.clone();
        std::fs::create_dir_all(game.join(".fishy-update").join("u")).unwrap();
        std::fs::write(game.join(".fishy-update").join("u").join("big.pak"), b"new").unwrap();
        std::fs::write(game.join("big.pak"), b"old").unwrap();
        tauri::async_runtime::block_on(d.discard(u.info.clone())).unwrap();
        assert!(!game.join(".fishy-update").exists());
        assert_eq!(std::fs::read(game.join("big.pak")).unwrap(), b"old", "the game keeps its version");
    }

    #[test]
    fn a_cancel_that_comes_too_late_lets_the_update_finish() {
        assert!(discard_after_run(true, false, true), "an install that was cancelled goes");
        assert!(discard_after_run(true, true, false), "an update stopped before the move goes");
        assert!(!discard_after_run(true, true, true), "an update that moved its files is done");
        assert!(!discard_after_run(false, true, false));
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
    fn begin_starts_the_named_job_under_the_same_lock() {
        let mut inner = Inner { jobs: vec![job("p", JobState::Paused), job("q1", JobState::Queued), job("q2", JobState::Queued)], wiping: false };
        let started = inner.begin("q2").expect("q2 waits, so it starts");
        assert_eq!(started.info.app_name, "q2");
        assert_eq!(inner.jobs[2].state, JobState::Downloading);
        assert!(inner.jobs[2].stop.as_ref().is_some_and(|s| !s.is_cancelled()));
        assert!(inner.begin("q2").is_none(), "already downloading");
        assert!(inner.begin("p").is_none(), "paused jobs don't start");
        assert!(inner.begin("gone").is_none());
    }

    #[test]
    fn begin_next_starts_the_first_waiting_job_only_when_nothing_runs() {
        let mut inner = Inner { jobs: vec![job("p", JobState::Paused), job("q1", JobState::Queued), job("q2", JobState::Queued)], wiping: false };
        assert_eq!(inner.begin_next().map(|s| s.info.app_name), Some("q1".to_string()));
        assert!(inner.begin_next().is_none(), "one at a time");
    }

    #[test]
    fn start_now_both_starts_the_chosen_game_by_name() {
        let mut inner = Inner { jobs: vec![job("run", JobState::Downloading), job("w", JobState::Queued)], wiping: false };
        let started = inner.start_now_begin("w", StartNow::Both).unwrap();
        assert_eq!(started.map(|s| s.info.app_name), Some("w".to_string()));
        assert_eq!(inner.jobs[1].state, JobState::Downloading);
    }

    #[test]
    fn start_now_with_nothing_running_starts_it_at_once() {
        let mut inner = Inner { jobs: vec![job("a", JobState::Queued), job("w", JobState::Queued)], wiping: false };
        let started = inner.start_now_begin("w", StartNow::Pause).unwrap();
        assert_eq!(started.map(|s| s.info.app_name), Some("w".to_string()));
        assert_eq!(inner.jobs[0].info.app_name, "w");
    }

    #[test]
    fn uninstall_refusal_names_what_is_in_the_way() {
        let mut repair = job("r", JobState::Queued);
        repair.info.imported = true;
        let inner = Inner { jobs: vec![update("run", JobState::Downloading), update("q", JobState::Queued), update("p", JobState::Paused), repair], wiping: false };
        assert_eq!(inner.uninstall_refusal("run"), Some("Pause the update first, then uninstall."));
        assert_eq!(inner.uninstall_refusal("q"), Some("Pause the update first, then uninstall."));
        assert_eq!(inner.uninstall_refusal("p"), None, "a paused update goes with the game");
        assert_eq!(inner.uninstall_refusal("r"), Some("Finish or cancel its download first, then uninstall."));
        assert_eq!(inner.uninstall_refusal("none"), None);
    }

    #[test]
    fn a_finished_download_tells_who_asked() {
        let (_dir, d) = downloads_with(vec![job("a", JobState::Downloading)]);
        let seen = Arc::new(Mutex::new(Vec::<String>::new()));
        let s = seen.clone();
        d.when_finished(move |name| s.lock().unwrap().push(name.to_string()));
        d.finished_ok("a");
        assert!(d.lock().find("a").is_none(), "the job leaves the list");
        assert_eq!(*seen.lock().unwrap(), vec!["a".to_string()]);
    }

    /// Fortnite in small (spec §6u): the game lists every file, "Lego" lists its two. Every file
    /// is on disk; each has a paused update.
    fn fortnite_with_lego() -> (tempfile::TempDir, Downloads) {
        use fishy_core::install::manifest::testbuild::{build_binary, sample};
        use fishy_core::install::manifest::{ChunkPart, FileEntry};
        let file = |name: &str, tag: Option<&str>| FileEntry {
            install_tags: tag.map(|t| vec![t.to_string()]).unwrap_or_default(),
            filename: name.into(),
            sha1: [3; 20],
            executable: false,
            parts: vec![ChunkPart { guid: [1, 2, 3, 4], offset: 0, size: 10 }],
        };
        let m = |files: Vec<FileEntry>| {
            let s = sample();
            Manifest::new(s.meta, s.chunks, files)
        };
        let game = m(vec![file("Fortnite.exe", None), file("br.pak", Some("BR")), file("juno.pak", Some("Juno"))]);
        let lego = m(vec![file("juno.pak", Some("Juno"))]);
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Fortnite");
        std::fs::create_dir_all(&root).unwrap();
        for f in &game.files {
            std::fs::write(root.join(&f.filename), b"x").unwrap();
        }
        let store = InstallStore::at(dir.path().join("data"));
        for (app, parts, parent, man) in [("Fortnite", vec!["BR", "Juno"], None, &game), ("Lego", vec!["Juno"], Some("Fortnite"), &lego)] {
            let mut info = job(app, JobState::Paused).info;
            info.install_path = root.clone();
            info.imported = true;
            info.parts = Some(parts.iter().map(|p| p.to_string()).collect());
            info.parent = parent.map(str::to_string);
            store.save_job(&info, &build_binary(man, true)).unwrap();
            store.finish_install(&info, man).unwrap();
            info.update_from = Some("1".into());
            store.save_job(&info, &build_binary(man, true)).unwrap();
        }
        let d = Downloads::new(store, InstallClient::new());
        (dir, d)
    }

    #[test]
    fn removing_a_piece_drops_its_update_and_narrows_the_games() {
        let (dir, d) = fortnite_with_lego();
        assert_eq!(d.lock().jobs[0].manifest.files.len(), 3, "the game's update has all three files");
        d.remove_piece("Lego").unwrap();
        assert!(!dir.path().join("Fortnite").join("juno.pak").exists());
        let inner = d.lock();
        assert_eq!(inner.jobs.iter().map(|j| j.info.app_name.as_str()).collect::<Vec<_>>(), vec!["Fortnite"]);
        assert_eq!(inner.jobs[0].info.parts, Some(vec!["BR".to_string()]));
        assert_eq!(inner.jobs[0].manifest.files.len(), 2, "juno.pak isn't fetched any more");
        assert_eq!(inner.jobs[0].state, JobState::Paused);
    }

    #[test]
    fn a_piece_isnt_removed_while_its_game_downloads() {
        let (dir, d) = fortnite_with_lego();
        d.lock().jobs[1].state = JobState::Downloading; // the piece's update runs
        assert_eq!(d.remove_piece("Lego").unwrap_err(), PIECE_BUSY);
        d.lock().jobs[1].state = JobState::Paused;
        d.lock().jobs[0].state = JobState::Downloading; // the game's runs
        assert_eq!(d.remove_piece("Lego").unwrap_err(), PIECE_BUSY);
        assert!(dir.path().join("Fortnite").join("juno.pak").exists());
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
