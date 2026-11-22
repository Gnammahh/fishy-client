//! Fishy Client's own updates (spec §6j): checked once at start-up, installed only after the user
//! presses Update now, signed with a key only the maintainer has.
use crate::AppInstall;
use fishy_core::settings::Settings;
use serde::Serialize;
use tauri::{Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;
use tracing::{info, warn};

pub(crate) const DOWNLOAD_ERROR: &str = "Couldn't download the update. Try again later.";

/// A newer version on offer, as the update bar shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AppUpdate {
    pub version: String,
    pub notes: String,
}

/// True when `offered` is a newer version than `current`. Anything that isn't a version isn't newer.
pub(crate) fn newer_than(current: &str, offered: &str) -> bool {
    match (semver::Version::parse(current), semver::Version::parse(offered)) {
        (Ok(c), Ok(o)) => o > c,
        _ => false,
    }
}

pub(crate) fn should_check(s: &Settings) -> bool {
    s.check_app_updates
}

/// Update now waits while a game runs: the installer would have to close it.
pub(crate) fn install_blocked(running_titles: &[String]) -> Option<String> {
    running_titles.first().map(|t| fishy_core::running::close_first(t))
}

/// Whole percent for the progress line, never over 100; unknown size counts as 0.
pub(crate) fn percent(done: u64, total: Option<u64>) -> u8 {
    match total {
        Some(t) if t > 0 => (done.min(t) * 100 / t) as u8,
        _ => 0,
    }
}

fn settings(app: &tauri::AppHandle) -> Settings {
    app.state::<crate::updates::AppSettings>().store.load()
}

/// Asks GitHub once whether a newer Fishy Client exists. None when switched off, nothing newer,
/// or the check failed (only logged: nobody asked for it).
#[tauri::command]
pub async fn app_update_check(app: tauri::AppHandle) -> Option<AppUpdate> {
    if !should_check(&settings(&app)) {
        return None;
    }
    let current = app.package_info().version.to_string();
    let found = match app.updater() {
        Ok(u) => u.check().await,
        Err(e) => Err(e),
    };
    match found {
        Ok(Some(update)) if newer_than(&current, &update.version) => {
            info!("app update: {} available", update.version);
            Some(AppUpdate { version: update.version.clone(), notes: update.body.clone().unwrap_or_default() })
        }
        Ok(_) => None,
        Err(_) => {
            warn!("app update: check failed");
            None
        }
    }
}

#[derive(Clone, Serialize)]
struct Progress {
    percent: u8,
}

/// Update now: refuses while a game runs, pauses downloads, downloads and installs the signed
/// update (the plugin refuses a bad signature), then restarts.
#[tauri::command]
pub async fn app_update_install(
    app: tauri::AppHandle,
    install: tauri::State<'_, AppInstall>,
    running: tauri::State<'_, crate::running::AppRunning>,
) -> Result<(), String> {
    let installed = install.downloads.store.installed();
    let titles: Vec<String> =
        running.in_use().iter().map(|a| installed.iter().find(|g| &g.app_name == a).map(|g| g.title.clone()).unwrap_or_else(|| a.clone())).collect();
    if let Some(m) = install_blocked(&titles) {
        return Err(m);
    }
    // Downloads pause first, so they show as Paused after the restart like after any close.
    install.downloads.pause_all(&app);
    let update = match app.updater().map_err(|_| DOWNLOAD_ERROR.to_string())?.check().await {
        Ok(Some(u)) => u,
        _ => {
            warn!("app update: check failed");
            return Err(DOWNLOAD_ERROR.to_string());
        }
    };
    info!("app update: installing {}", update.version);
    let (mut done, mut last) = (0u64, 255u8);
    let progress_app = app.clone();
    let result = update
        .download_and_install(
            move |chunk, total| {
                done += chunk as u64;
                let p = percent(done, total);
                if p != last {
                    last = p;
                    let _ = progress_app.emit("app-update-progress", Progress { percent: p });
                }
            },
            || {},
        )
        .await;
    if result.is_err() {
        warn!("app update: check failed");
        return Err(DOWNLOAD_ERROR.to_string());
    }
    app.restart();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_than_compares_versions() {
        assert!(newer_than("1.1.0", "1.2.0"));
        assert!(newer_than("1.9.0", "1.10.0"));
        assert!(newer_than("1.1.0", "1.1.1"));
        assert!(!newer_than("1.1.0", "1.1.0"));
        assert!(!newer_than("1.2.0", "1.1.0"));
        assert!(!newer_than("1.1.0", "not a version"));
    }

    #[test]
    fn no_check_when_switched_off() {
        assert!(should_check(&Settings::default()));
        assert!(!should_check(&Settings { check_app_updates: false, ..Settings::default() }));
    }

    #[test]
    fn install_refused_while_a_game_runs() {
        assert_eq!(install_blocked(&[]), None);
        assert_eq!(install_blocked(&["Bendy".into(), "Sol Cesto".into()]), Some("Close Bendy first.".to_string()));
    }

    #[test]
    fn progress_is_a_whole_percent_that_never_passes_100() {
        assert_eq!(percent(0, Some(200)), 0);
        assert_eq!(percent(84, Some(200)), 42);
        assert_eq!(percent(500, Some(200)), 100);
        assert_eq!(percent(10, None), 0);
    }

    #[test]
    fn messages_say_what_to_do() {
        assert_eq!(DOWNLOAD_ERROR, "Couldn't download the update. Try again later.");
    }
}
