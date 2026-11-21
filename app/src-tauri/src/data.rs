//! Settings → Account: logs folder and your data (spec §6f.1).

use crate::{AppAuth, AppInstall, AppLibrary};
use fishy_core::logs;
use fishy_core::wipe::{default_data_root, delete_everything};
use serde::Serialize;
use tauri::Emitter;

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
) -> Result<DeleteReport, String> {
    tracing::info!("delete all my data (games too: {also_games})");
    // 1. Stop every download and forget prepared Install boxes.
    let jobs = install.downloads.take_jobs_for_wipe(&app).await;
    install.prepared.lock().unwrap_or_else(|e| e.into_inner()).clear();
    // 2. Log out, the same as the Log out button.
    let state = auth.manager.logout().await;
    let _ = library.cache.clear();
    auth.set(state).await;
    // 4. (before the files go, so a failure here can't be missed in the log) Start with Windows off.
    {
        use tauri_plugin_autostart::ManagerExt;
        let _ = app.autolaunch().disable();
    }
    // Task 8: reset the shared speed limiter here.
    // 3 and 5. Games (if ticked), then everything in %APPDATA%\FishyClient.
    let store = install.downloads.store.clone();
    let failed = tauri::async_runtime::spawn_blocking(move || delete_everything(&store, &default_data_root(), jobs, also_games))
        .await
        .unwrap_or(1);
    let _ = app.emit("installed-changed", ());
    tracing::info!("delete all my data finished, {failed} not deleted");
    Ok(DeleteReport { failed, message: delete_message(failed) })
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
    fn open_logs_error_is_plain() {
        assert_eq!(logs_error(), r"Couldn't open the logs folder. It's at %APPDATA%\FishyClient\logs.");
    }
}
