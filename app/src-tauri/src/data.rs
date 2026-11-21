//! Settings → Account: logs folder and your data (spec §6f.1).

use fishy_core::logs;

pub(crate) fn logs_error() -> String {
    r"Couldn't open the logs folder. It's at %APPDATA%\FishyClient\logs.".to_string()
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
    fn open_logs_error_is_plain() {
        assert_eq!(logs_error(), r"Couldn't open the logs folder. It's at %APPDATA%\FishyClient\logs.");
    }
}
