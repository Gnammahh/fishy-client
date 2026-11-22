//! Watching games started by Play (spec §6f.4). One check every 5 seconds, only while a game
//! is in the list. Program paths stay in memory: nothing about other programs is logged.
use fishy_core::running::{OpenPrograms, RunningList};
use std::path::Path;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

const CHECK_EVERY: Duration = Duration::from_secs(5);

#[derive(Default)]
struct Watch {
    list: RunningList,
    /// Games whose Play is between the click and the game starting.
    launching: std::collections::HashSet<String>,
    /// The check loop is running.
    watching: bool,
    /// Fishy Client minimized itself for a game, so it comes back when they've all closed.
    minimized_by_us: bool,
}

#[derive(Default)]
pub struct AppRunning {
    watch: std::sync::Mutex<Watch>,
}

impl AppRunning {
    fn lock(&self) -> std::sync::MutexGuard<'_, Watch> {
        self.watch.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn names(&self) -> Vec<String> {
        self.lock().list.names()
    }

    pub fn contains(&self, app_name: &str) -> bool {
        self.lock().list.contains(app_name)
    }

    /// Claims the game for one Play. None when it's already starting or running. The claim ends
    /// when the returned guard is dropped (the game is in the running list by then, or it failed).
    pub fn begin_launch(&self, app_name: &str) -> Option<Launching<'_>> {
        let mut w = self.lock();
        if w.list.contains(app_name) || !w.launching.insert(app_name.to_string()) {
            return None;
        }
        Some(Launching { running: self, app_name: app_name.to_string() })
    }
}

pub struct Launching<'a> {
    running: &'a AppRunning,
    app_name: String,
}

impl Drop for Launching<'_> {
    fn drop(&mut self) {
        self.running.lock().launching.remove(&self.app_name);
    }
}

/// The window comes back only when the last game has closed, and only if Fishy Client hid it.
fn should_restore(now_empty: bool, minimized_by_us: bool) -> bool {
    now_empty && minimized_by_us
}

/// The running games' code names, for the Library when it opens.
#[tauri::command]
pub fn running_list(running: tauri::State<'_, AppRunning>) -> Vec<String> {
    running.names()
}

/// Play started the game: watch it, tell the UI, and minimize if the setting says so.
pub fn started(app: &AppHandle, app_name: &str, folder: &Path, minimize: bool) {
    let state = app.state::<AppRunning>();
    let (names, start_loop) = {
        let mut w = state.lock();
        w.list.add(app_name, folder);
        w.minimized_by_us |= minimize;
        let start_loop = !w.watching;
        w.watching = true;
        (w.list.names(), start_loop)
    };
    tracing::info!("{app_name} is running");
    let _ = app.emit("running-changed", names);
    if let Some(window) = app.get_webview_window("main").filter(|_| minimize) {
        let _ = window.minimize();
    }
    if start_loop {
        tauri::async_runtime::spawn(watch(app.clone()));
    }
}

/// Resets the watch if the loop stops without finishing normally (a crash), so the next Play
/// starts a fresh check instead of never being watched.
struct ResetIfStopped<'a> {
    running: &'a AppRunning,
    finished: bool,
}

impl Drop for ResetIfStopped<'_> {
    fn drop(&mut self) {
        if !self.finished {
            let mut w = self.running.lock();
            w.list = RunningList::default();
            w.watching = false;
            w.minimized_by_us = false;
        }
    }
}

/// Every 5 seconds: drop the games with nothing open. Ends when the list is empty.
async fn watch(app: AppHandle) {
    let state = app.state::<AppRunning>();
    let mut guard = ResetIfStopped { running: state.inner(), finished: false };
    loop {
        tokio::time::sleep(CHECK_EVERY).await;
        let taken_at = std::time::Instant::now();
        // If the check itself fails, everything counts as closed, so no card stays stuck.
        let paths = tauri::async_runtime::spawn_blocking(|| OpenPrograms::new().paths()).await.unwrap_or_default();
        let (closed, names, restore, done) = {
            let mut w = state.lock();
            let closed = w.list.check(&paths, taken_at);
            let empty = w.list.is_empty();
            let restore = should_restore(empty, w.minimized_by_us);
            if empty {
                w.watching = false;
                w.minimized_by_us = false;
            }
            (closed, w.list.names(), restore, empty)
        };
        for name in &closed {
            tracing::info!("{name} closed");
        }
        if !closed.is_empty() {
            let _ = app.emit("running-changed", names);
        }
        if restore {
            bring_back(&app);
        }
        if done {
            guard.finished = true;
            return;
        }
    }
}

/// Windows may refuse the focus and flash the taskbar button instead; that's fine.
fn bring_back(app: &AppHandle) {
    // Only if it's still minimized: if you brought it back yourself, it stays where you put it.
    if let Some(window) = app.get_webview_window("main").filter(|w| w.is_minimized().unwrap_or(false)) {
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_only_when_empty_and_we_minimized() {
        assert!(should_restore(true, true));
        assert!(!should_restore(false, true), "another game is still running");
        assert!(!should_restore(true, false), "the user chose Keep open");
    }

    #[test]
    fn a_second_play_is_refused_while_the_first_is_starting_or_running() {
        let r = AppRunning::default();
        let first = r.begin_launch("kelp").expect("first Play goes ahead");
        assert!(r.begin_launch("kelp").is_none(), "still starting");
        assert!(r.begin_launch("reef").is_some(), "another game is fine");
        drop(first); // the launch failed
        assert!(r.begin_launch("kelp").is_some(), "allowed again after a failure");
        r.lock().list.add("kelp", std::path::Path::new(r"C:\Games\Kelp"));
        assert!(r.begin_launch("kelp").is_none(), "running");
    }

    #[test]
    fn a_crashed_check_resets_the_watch() {
        let r = AppRunning::default();
        {
            let mut w = r.lock();
            w.list.add("kelp", std::path::Path::new(r"C:\Games\Kelp"));
            w.watching = true;
        }
        {
            let _guard = ResetIfStopped { running: &r, finished: false };
            // the loop panics here
        }
        let w = r.lock();
        assert!(!w.watching && w.list.is_empty(), "the next Play starts a fresh check");
    }

    #[test]
    fn contains_follows_the_list() {
        let r = AppRunning::default();
        r.lock().list.add("kelp", Path::new(r"C:\Games\Kelp"));
        assert!(r.contains("kelp"));
        assert!(!r.contains("reef"));
    }
}
