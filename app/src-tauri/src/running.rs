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

    pub fn contains(&self, app_name: &str) -> bool {
        self.lock().list.contains(app_name)
    }
}

/// The window comes back only when the last game has closed, and only if Fishy Client hid it.
fn should_restore(now_empty: bool, minimized_by_us: bool) -> bool {
    now_empty && minimized_by_us
}

/// The running games' code names, for the Library when it opens.
#[tauri::command]
pub fn running_list(running: tauri::State<'_, AppRunning>) -> Vec<String> {
    running.lock().list.names()
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

/// Every 5 seconds: drop the games with nothing open. Ends when the list is empty.
async fn watch(app: AppHandle) {
    let mut programs = Some(OpenPrograms::new());
    loop {
        tokio::time::sleep(CHECK_EVERY).await;
        let mut p = programs.take().unwrap_or_default();
        let (back, paths) = match tauri::async_runtime::spawn_blocking(move || {
            let paths = p.paths();
            (p, paths)
        })
        .await
        {
            Ok((p, paths)) => (Some(p), paths),
            Err(_) => (None, Vec::new()), // the check itself failed: treat everything as closed
        };
        programs = back;
        let state = app.state::<AppRunning>();
        let (closed, names, restore, done) = {
            let mut w = state.lock();
            let closed = w.list.check(&paths);
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
            return;
        }
    }
}

/// Windows may refuse the focus and flash the taskbar button instead; that's fine.
fn bring_back(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
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
    fn contains_follows_the_list() {
        let r = AppRunning::default();
        r.lock().list.add("kelp", Path::new(r"C:\Games\Kelp"));
        assert!(r.contains("kelp"));
        assert!(!r.contains("reef"));
    }
}
