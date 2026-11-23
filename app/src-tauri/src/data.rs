//! Settings → Account: logs folder and your data (spec §6f.1).

use crate::{AppAuth, AppInstall, AppLibrary};
use fishy_core::logs;
use fishy_core::wipe::{default_data_root, delete_everything};
use serde::Serialize;
use tauri::Emitter;

/// Delete all my data, step 4: true if Start with Windows is on and couldn't be turned off.
/// If its state can't be read, it's turned off anyway.
pub(crate) fn autostart_off_failed<E>(enabled: Result<bool, E>, disable: impl FnOnce() -> Result<(), E>) -> bool {
    enabled.unwrap_or(true) && disable().is_err()
}

/// Empties the app window's WebView2 data (cached cover images and the like, in
/// `%LOCALAPPDATA%\com.fishyclient.app`). True when it couldn't. No window: nothing to clear.
pub(crate) fn browser_data_failed<E>(clear: Option<impl FnOnce() -> Result<(), E>>) -> bool {
    clear.is_some_and(|c| c().is_err())
}

pub(crate) const STOP_ERROR: &str = "A download wouldn't stop, so nothing was deleted. Try again in a minute.";

pub(crate) const AUTOSTART_ERROR: &str = "Couldn't change Start with Windows.";

/// Start with Windows, read from the Registry each time (never saved in settings.json).
#[tauri::command]
pub fn autostart_get(app: tauri::AppHandle) -> bool {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().unwrap_or(false)
}

/// Turns Start with Windows on or off. Returns the real state afterwards.
#[tauri::command]
pub fn autostart_set(app: tauri::AppHandle, on: bool) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    let launcher = app.autolaunch();
    let result = if on { launcher.enable() } else { launcher.disable() };
    if result.is_err() {
        tracing::warn!("{AUTOSTART_ERROR}");
        return Err(AUTOSTART_ERROR.to_string());
    }
    tracing::info!("start with Windows: {on}");
    Ok(launcher.is_enabled().unwrap_or(on))
}

/// The built-in default install folder, `%USERPROFILE%\Games`, for Settings to show.
#[tauri::command]
pub fn default_install_folder() -> String {
    fishy_core::install::space::default_install_root().display().to_string()
}

/// Shown in the Install box when the saved default folder's drive is missing.
pub(crate) fn unavailable_folder_note(used: &std::path::Path) -> String {
    format!("Your default folder isn't available, so this uses {}.", used.display())
}

pub(crate) fn logs_error() -> String {
    r"Couldn't open the logs folder. It's at %APPDATA%\FishyClient\logs.".to_string()
}

/// What's left after Delete all my data. `message` is set when something couldn't be deleted.
#[derive(Debug, Serialize)]
pub struct DeleteReport {
    pub failed: usize,
    pub message: Option<String>,
}

pub(crate) fn delete_message(failed: usize) -> Option<String> {
    (failed > 0).then(|| format!("Some files couldn't be deleted ({failed}). Close any running games and try again."))
}

/// Settings → Delete all my data (spec §6f.1). Partial failures still return Ok, with a message.
#[tauri::command]
pub async fn delete_all_data(
    app: tauri::AppHandle,
    also_games: bool,
    auth: tauri::State<'_, AppAuth>,
    library: tauri::State<'_, AppLibrary>,
    install: tauri::State<'_, AppInstall>,
    running: tauri::State<'_, crate::running::AppRunning>,
) -> Result<DeleteReport, String> {
    tracing::info!("delete all my data (games too: {also_games})");
    games_free_to_delete(also_games, &running.in_use())?;
    // 1. Stop every download and forget prepared Install boxes.
    let jobs = install.downloads.stop_for_wipe(std::time::Duration::from_secs(60)).await.map_err(|_| {
        tracing::warn!("{STOP_ERROR}");
        let _ = app.emit("downloads-changed", install.downloads.view());
        STOP_ERROR.to_string()
    })?;
    let _ = app.emit("downloads-changed", install.downloads.view());
    install.prepared.lock().unwrap_or_else(|e| e.into_inner()).clear();
    // 2. Log out, the same as the Log out button.
    let state = auth.manager.logout().await;
    let _ = library.cache.clear();
    auth.set(state).await;
    let autostart_failed;
    // 4. (before the files go, so a failure here can't be missed in the log) Start with Windows off.
    {
        use tauri_plugin_autostart::ManagerExt;
        let launcher = app.autolaunch();
        autostart_failed = autostart_off_failed(launcher.is_enabled(), || launcher.disable());
        if autostart_failed {
            tracing::warn!("{AUTOSTART_ERROR}");
        }
    }
    // The Store window closes first (spec §6o); the browser data below is its sign-in too.
    crate::store_window::close_quietly(&app);
    // The window's own browser data goes too (cover images it cached, for one).
    let browser_failed = {
        use tauri::Manager;
        let window = app.get_webview_window("main");
        browser_data_failed(window.as_ref().map(|w| move || w.clear_all_browsing_data()))
    };
    if browser_failed {
        tracing::warn!("couldn't clear the window's browser data");
    }
    install.downloads.limiter.set(None); // the saved limit is deleted too
    // 3 and 5. Games (if ticked), then everything in %APPDATA%\FishyClient.
    let store = install.downloads.store.clone();
    let failed = tauri::async_runtime::spawn_blocking(move || delete_everything(&store, &default_data_root(), jobs, also_games))
        .await
        .unwrap_or(1)
        + usize::from(autostart_failed)
        + usize::from(browser_failed);
    install.downloads.end_wipe();
    let _ = app.emit("installed-changed", ());
    tracing::info!("delete all my data finished, {failed} not deleted");
    Ok(DeleteReport { failed, message: delete_message(failed) })
}

/// The first `max_chars` characters of `s` (never splitting one).
pub(crate) fn cut(s: &str, max_chars: usize) -> &str {
    s.char_indices().nth(max_chars).map_or(s, |(i, _)| &s[..i])
}

const GAMES_RUNNING: &str = "Close your running games first, then try again.";

/// Deleting the games too isn't possible while one of them runs (its files are in use).
fn games_free_to_delete(also_games: bool, running: &[String]) -> Result<(), String> {
    if also_games && !running.is_empty() {
        return Err(GAMES_RUNNING.to_string());
    }
    Ok(())
}

/// A bug in a screen (spec §6f.2). The text goes through the log's secret scrubber like
/// every other line.
#[tauri::command]
pub fn log_ui_error(message: String) {
    tracing::warn!("ui error: {}", ui_line(&message));
}

/// At most 500 characters, on one line (a stack trace's line breaks become " | ").
fn ui_line(message: &str) -> String {
    cut(message, 500).lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" | ")
}

/// Keeps the log writer alive (and flushing) for as long as the app runs.
pub struct AppLogs(#[allow(dead_code)] pub Option<logs::LogGuard>);

/// Opens `%APPDATA%\FishyClient\logs` in File Explorer. Only this one fixed path, no plugin.
#[tauri::command]
pub fn open_logs_folder() -> Result<(), String> {
    let dir = logs::default_dir();
    std::fs::create_dir_all(&dir).map_err(|_| logs_error())?;
    std::process::Command::new("explorer.exe").arg(&dir).spawn().map_err(|_| logs_error())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn games_are_not_deleted_while_one_runs() {
        let kelp = vec!["kelp".to_string()];
        assert_eq!(games_free_to_delete(true, &kelp), Err(GAMES_RUNNING.to_string()));
        assert_eq!(games_free_to_delete(false, &kelp), Ok(()), "settings only: the game keeps running");
        assert_eq!(games_free_to_delete(true, &[]), Ok(()));
    }

    #[test]
    fn a_screen_error_is_one_line() {
        assert_eq!(ui_line("TypeError: x\n    at a.js:1\r\n    at b.js:2"), "TypeError: x | at a.js:1 | at b.js:2");
        assert_eq!(ui_line("plain"), "plain");
    }

    #[test]
    fn cut_keeps_whole_characters() {
        let long = "aé".repeat(400);
        let c = cut(&long, 500);
        assert_eq!(c.chars().count(), 500);
        assert!(long.starts_with(c));
        assert_eq!(cut("short", 500), "short");
    }

    #[test]
    fn delete_message_wording() {
        assert_eq!(delete_message(0), None);
        assert_eq!(delete_message(3).as_deref(), Some("Some files couldn't be deleted (3). Close any running games and try again."));
    }

    #[test]
    fn unavailable_folder_note_names_the_folder_used() {
        assert_eq!(
            unavailable_folder_note(std::path::Path::new(r"C:\Users\you\Games")),
            r"Your default folder isn't available, so this uses C:\Users\you\Games."
        );
    }

    #[test]
    fn autostart_turn_off_failure_is_counted() {
        let mut asked = false;
        assert!(autostart_off_failed(Ok(true), || {
            asked = true;
            Err(())
        }));
        assert!(asked);
        assert!(!autostart_off_failed::<()>(Ok(true), || Ok(())));
    }

    #[test]
    fn the_window_browser_data_is_cleared_and_a_failure_counts() {
        assert!(!browser_data_failed(Some(|| Ok::<(), ()>(()))));
        assert!(browser_data_failed(Some(|| Err::<(), ()>(()))));
        assert!(!browser_data_failed(None::<fn() -> Result<(), ()>>), "no window, nothing to clear");
        assert!(!autostart_off_failed::<()>(Ok(false), || panic!("not on, so not touched")));
        assert!(!autostart_off_failed::<()>(Err(()), || Ok(())), "can't read it: try to turn it off anyway");
    }

    #[test]
    fn autostart_error_is_plain() {
        assert_eq!(AUTOSTART_ERROR, "Couldn't change Start with Windows.");
    }

    #[test]
    fn open_logs_error_is_plain() {
        assert_eq!(logs_error(), r"Couldn't open the logs folder. It's at %APPDATA%\FishyClient\logs.");
    }
}
