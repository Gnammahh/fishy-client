//! `--play <game>` from a game's shortcut (spec §6t).
use crate::AppInstall;
use fishy_core::install::store::InstalledGame;
use std::sync::Mutex;
use tauri::{Emitter, Manager};

/// The game named after `--play`, if any.
pub fn play_arg(args: &[String]) -> Option<String> {
    let i = args.iter().position(|a| a == "--play")?;
    args.get(i + 1).filter(|n| !n.is_empty() && !n.starts_with("--")).cloned()
}

/// Only installed games are played; anything else just opens Fishy Client.
pub fn wanted(name: &str, installed: &[InstalledGame]) -> bool {
    installed.iter().any(|g| g.app_name == name)
}

/// A game a shortcut asked to play, until the page takes it.
#[derive(Default)]
pub struct AppPlayRequest(Mutex<Option<String>>);

/// Fishy's own start-up arguments, or those a second start handed over.
pub fn accept(app: &tauri::AppHandle, args: &[String]) {
    let Some(name) = play_arg(args) else { return };
    if !wanted(&name, &app.state::<AppInstall>().downloads.store.installed()) {
        tracing::info!("play request for a game that isn't installed");
        return;
    }
    tracing::info!("play request from shortcut");
    *app.state::<AppPlayRequest>().0.lock().unwrap_or_else(|p| p.into_inner()) = Some(name.clone());
    let _ = app.emit("play-request", name);
}

/// A second start of Fishy Client: show the open window, then handle its arguments.
pub fn second_start(app: &tauri::AppHandle, args: Vec<String>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
    accept(app, &args);
}

/// The page takes the waiting request (once).
#[tauri::command]
pub fn play_request_take(request: tauri::State<'_, AppPlayRequest>) -> Option<String> {
    request.0.lock().unwrap_or_else(|p| p.into_inner()).take()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn play_arg_parsing() {
        assert_eq!(play_arg(&args(&["fishy.exe", "--play", "Tide"])), Some("Tide".into()));
        assert_eq!(play_arg(&args(&["fishy.exe", "--minimized", "--play", "Tide Pool"])), Some("Tide Pool".into()));
        assert_eq!(play_arg(&args(&["fishy.exe", "--play"])), None);
        assert_eq!(play_arg(&args(&["fishy.exe"])), None);
        assert_eq!(play_arg(&args(&["x", "--play", ""])), None);
        assert_eq!(play_arg(&args(&["x", "--play", "--other"])), None);
    }

    #[test]
    fn play_request_for_unknown_game_is_ignored() {
        let g: InstalledGame = serde_json::from_value(serde_json::json!({
            "app_name": "Tide", "title": "T", "version": "1", "install_path": "C:/Games/T",
            "executable": "t.exe", "launch_parameters": "", "install_size": 1,
            "installed_at": "2026-11-20T10:00:00Z"
        }))
        .unwrap();
        assert!(wanted("Tide", std::slice::from_ref(&g)));
        assert!(!wanted("Reef", &[g]));
    }
}
