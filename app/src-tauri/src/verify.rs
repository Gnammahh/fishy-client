//! Verify files and the repair it offers, and the Manage box's options (spec §6t).
use crate::downloads::DownloadsView;
use crate::{AppAuth, AppInstall, AppLibrary};
use fishy_core::epic_import::{save_repair, FileCheck};
use fishy_core::install::manifest::parse_manifest;
use fishy_core::install::parts::only_parts;
use fishy_core::install::store::{check_launch_args, InstalledGame, JobInfo};
use fishy_core::install::InstallError;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::Emitter;

const REPAIR_FETCH_ERROR: &str = "Couldn't reach Epic. Check your internet, then try again.";
const UPDATE_WAITING: &str = "An update is waiting. Update the game, then verify again.";
const NOTHING_TO_REPAIR: &str = "Verify the game again, then try again.";
/// Progress events at most this often (about 4 a second).
const EMIT_EVERY: Duration = Duration::from_millis(250);

/// The check that's running (at most one) and the last bad result of each game.
#[derive(Default)]
pub struct AppVerify {
    current: Mutex<Option<(String, Arc<AtomicBool>)>>,
    bad: Mutex<HashMap<String, FileCheck>>,
}

impl AppVerify {
    fn lock_current(&self) -> std::sync::MutexGuard<'_, Option<(String, Arc<AtomicBool>)>> {
        self.current.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Stops a running check of this game (Uninstall, Stop).
    pub fn stop(&self, app_name: &str) {
        if let Some((app, stop)) = self.lock_current().as_ref()
            && app == app_name {
                stop.store(true, Ordering::Relaxed);
            }
    }

    /// Delete all my data: stops whatever check runs and waits (up to `timeout`) until it has.
    /// False if it was still running at the end.
    pub async fn stop_all(&self, timeout: Duration) -> bool {
        if let Some((_, stop)) = self.lock_current().as_ref() {
            stop.store(true, Ordering::Relaxed);
        }
        let deadline = Instant::now() + timeout;
        while self.lock_current().is_some() {
            if Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        true
    }

    /// The game's files changed (an update or repair finished): its last result no longer holds.
    pub fn forget(&self, app_name: &str) {
        self.bad.lock().unwrap_or_else(|p| p.into_inner()).remove(app_name);
    }
}

/// Why Verify can't start now, if it can't.
pub fn verify_refusal(running: bool, has_job: bool, busy_other: bool, wiping_or_checking: bool) -> Option<&'static str> {
    if running {
        Some("Close the game first.")
    } else if has_job || wiping_or_checking {
        Some("Finish or cancel its download first.")
    } else if busy_other {
        Some("Another game is being checked.")
    } else {
        None
    }
}

/// A repair downloads from Epic's newest build, so it needs the installed version to be that build.
pub fn repair_needs_same_version(installed: &str, epic: &str) -> Result<(), &'static str> {
    if installed == epic {
        Ok(())
    } else {
        Err(UPDATE_WAITING)
    }
}

#[derive(Clone, Serialize)]
struct VerifyProgress {
    app_name: String,
    done: usize,
    total: usize,
}

#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum VerifyResult {
    Good,
    Bad { files: usize, bytes: u64 },
    Stopped,
    Error { message: String },
}

#[derive(Clone, Serialize)]
struct VerifyDone {
    app_name: String,
    result: VerifyResult,
}

fn installed_record(install: &AppInstall, app_name: &str) -> Result<InstalledGame, String> {
    install.downloads.store.installed().into_iter().find(|g| g.app_name == app_name).ok_or_else(|| InstallError::NotInstalled.user_message())
}

/// Saves the Manage box's switches and launch options (spec §6t).
#[tauri::command]
pub fn game_options_set(
    app: tauri::AppHandle,
    app_name: String,
    auto_update: Option<bool>,
    priority: bool,
    launch_args: String,
    launch_args_on: bool,
    install: tauri::State<'_, AppInstall>,
) -> Result<InstalledGame, String> {
    check_launch_args(&launch_args)?;
    let saved = install
        .downloads
        .store
        .set_options(&app_name, auto_update, priority, &launch_args, launch_args_on)
        .map_err(|e| InstallError::Io { folder: String::new(), detail: e.to_string() }.user_message())?
        .ok_or_else(|| InstallError::NotInstalled.user_message())?;
    install.downloads.set_priority(&app, &app_name, priority);
    tracing::info!("options saved");
    let _ = app.emit("installed-changed", ());
    Ok(saved)
}

/// Why a piece can't be removed now, if it can't (spec §6u). Downloads refuse on their own.
pub fn piece_refusal(running: bool, checking: bool) -> Option<&'static str> {
    if running {
        Some("Close the game first.")
    } else if checking {
        Some("Wait for the file check to finish.")
    } else {
        None
    }
}

/// Extra content → Remove (spec §6u): deletes an add-on piece such as LEGO Fortnite. Returns
/// the installed games afterwards.
#[tauri::command]
pub async fn piece_remove(
    app: tauri::AppHandle,
    app_name: String,
    install: tauri::State<'_, AppInstall>,
    running: tauri::State<'_, crate::running::AppRunning>,
    verify: tauri::State<'_, Arc<AppVerify>>,
) -> Result<Vec<InstalledGame>, String> {
    let rec = installed_record(&install, &app_name)?;
    let game = rec.parent.clone().ok_or_else(|| InstallError::NotInstalled.user_message())?;
    let checked = verify.lock_current().as_ref().is_some_and(|(a, _)| *a == game || *a == app_name);
    let refusal = piece_refusal(running.contains(&game) || running.contains(&app_name), checked || install.checking.is_busy(&game));
    if let Some(r) = refusal {
        return Err(r.into());
    }
    let downloads = install.downloads.clone();
    let name = app_name.clone();
    let result = tauri::async_runtime::spawn_blocking(move || downloads.remove_piece(&name)).await.map_err(|e| e.to_string())?;
    let _ = app.emit("downloads-changed", install.downloads.view());
    let _ = app.emit("installed-changed", ());
    if let Err(m) = &result {
        tracing::warn!("removing a piece failed: {m}");
    }
    result?;
    Ok(install.downloads.store.installed())
}

/// Starts checking a game's files in the background; progress comes as events.
#[tauri::command]
pub fn verify_start(
    app: tauri::AppHandle,
    app_name: String,
    install: tauri::State<'_, AppInstall>,
    running: tauri::State<'_, crate::running::AppRunning>,
    verify: tauri::State<'_, Arc<AppVerify>>,
) -> Result<(), String> {
    let rec = installed_record(&install, &app_name)?;
    let downloads = &install.downloads;
    let stop = Arc::new(AtomicBool::new(false));
    {
        let mut current = verify.lock_current();
        let refusal = verify_refusal(
            running.contains(&app_name),
            downloads.has_job(&app_name),
            current.is_some(),
            downloads.is_wiping() || install.checking.is_busy(&app_name),
        );
        if let Some(r) = refusal {
            return Err(r.into());
        }
        *current = Some((app_name.clone(), stop.clone()));
    }
    let store = downloads.store.clone();
    let verify = verify.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result = match store.installed_manifest(&app_name).ok().and_then(|b| parse_manifest(&b).ok()) {
            None => VerifyResult::Error { message: NOTHING_TO_REPAIR.into() },
            Some(full) => {
                let m = only_parts(full, rec.parts.as_deref());
                let mut last = Instant::now() - EMIT_EVERY;
                let mut on_progress = |done: usize, total: usize| {
                    if done == total || last.elapsed() >= EMIT_EVERY {
                        last = Instant::now();
                        let _ = app.emit("verify-progress", VerifyProgress { app_name: app_name.clone(), done, total });
                    }
                };
                match fishy_core::install::verify::verify(&rec.install_path, &m, &stop, &mut on_progress) {
                    None => VerifyResult::Stopped,
                    Some(check) => {
                        tracing::info!("verify: {} of {} files bad", check.bad_files, m.files.len());
                        let mut bad = verify.bad.lock().unwrap_or_else(|p| p.into_inner());
                        if check.all_good() {
                            bad.remove(&app_name);
                            VerifyResult::Good
                        } else {
                            let r = VerifyResult::Bad { files: check.bad_files, bytes: check.bad_bytes };
                            bad.insert(app_name.clone(), check);
                            r
                        }
                    }
                }
            }
        };
        *verify.lock_current() = None;
        let _ = app.emit("verify-done", VerifyDone { app_name, result });
    });
    Ok(())
}

#[tauri::command]
pub fn verify_stop(app_name: String, verify: tauri::State<'_, Arc<AppVerify>>) {
    verify.stop(&app_name);
}

/// Download them: queues only the missing or damaged files the last check found.
#[tauri::command]
pub async fn verify_repair(
    app: tauri::AppHandle,
    app_name: String,
    auth: tauri::State<'_, AppAuth>,
    library: tauri::State<'_, AppLibrary>,
    install: tauri::State<'_, AppInstall>,
    running: tauri::State<'_, crate::running::AppRunning>,
    verify: tauri::State<'_, Arc<AppVerify>>,
) -> Result<DownloadsView, String> {
    let result = queue_repair(&app, &app_name, &auth, &library, &install, &running, &verify).await;
    if let Err(m) = &result {
        tracing::warn!("repair of {app_name} refused");
        if m == crate::session::EXPIRED {
            crate::session::expire(&app).await;
        }
    }
    result
}

async fn queue_repair(
    app: &tauri::AppHandle,
    app_name: &str,
    auth: &AppAuth,
    library: &AppLibrary,
    install: &AppInstall,
    running: &crate::running::AppRunning,
    verify: &AppVerify,
) -> Result<DownloadsView, String> {
    let downloads = &install.downloads;
    let rec = installed_record(install, app_name)?;
    let check = verify.bad.lock().unwrap_or_else(|p| p.into_inner()).get(app_name).cloned().ok_or(NOTHING_TO_REPAIR)?;
    let game = library.cache.load().and_then(|c| c.games.into_iter().find(|g| g.app_name == app_name));
    let (namespace, catalog_item_id) = match game {
        Some(g) => (g.namespace, g.catalog_item_id),
        None => (rec.namespace.clone(), rec.catalog_item_id.clone()),
    };
    let token = auth.manager.access_token().await.map_err(|e| crate::updates::update_error(e.into()))?;
    let info = downloads.client.manifest_info(&token, &namespace, &catalog_item_id, app_name).await.map_err(repair_error)?;
    repair_needs_same_version(&rec.version, &info.build_version)?;
    let bytes = downloads.store.installed_manifest(app_name).map_err(|_| NOTHING_TO_REPAIR.to_string())?;
    let m = only_parts(parse_manifest(&bytes).map_err(|_| NOTHING_TO_REPAIR.to_string())?, rec.parts.as_deref());
    // Nothing may have started on this game while Epic was asked.
    let refusal = verify_refusal(running.contains(app_name), downloads.has_job(app_name), false, downloads.is_wiping() || install.checking.is_busy(app_name));
    if let Some(r) = refusal {
        return Err(r.into());
    }
    let job = JobInfo {
        app_name: rec.app_name.clone(),
        title: rec.title.clone(),
        namespace,
        catalog_item_id,
        install_path: rec.install_path.clone(),
        download_size: m.chunks.iter().map(|c| c.file_size).sum(),
        install_size: m.install_size(),
        base_urls: info.base_urls,
        update_from: None,
        // Cancel must leave the game's files alone, as for an import's repair (spec §6g).
        imported: true,
        parts: rec.parts.clone(),
        parent: rec.parent.clone(),
    };
    if let Err(e) = save_repair(&downloads.store, &job, &bytes, &check) {
        let _ = downloads.store.remove_job(app_name);
        tracing::warn!("repair of {app_name} couldn't be saved: {}", e.kind());
        return Err(REPAIR_FETCH_ERROR.into());
    }
    verify.bad.lock().unwrap_or_else(|p| p.into_inner()).remove(app_name);
    tracing::info!("repair of {app_name} queued ({} files)", check.bad_files);
    downloads.enqueue(app, job, Arc::new(m), None);
    Ok(downloads.view())
}

fn repair_error(e: InstallError) -> String {
    match e {
        InstallError::Expired => crate::session::EXPIRED.to_string(),
        InstallError::Rejected(_) | InstallError::Unsupported => e.user_message(),
        _ => REPAIR_FETCH_ERROR.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_refusal_reasons_in_order() {
        assert_eq!(verify_refusal(false, false, false, false), None);
        assert_eq!(verify_refusal(true, true, true, true), Some("Close the game first."));
        assert_eq!(verify_refusal(false, true, true, true), Some("Finish or cancel its download first."));
        assert_eq!(verify_refusal(false, false, true, false), Some("Another game is being checked."));
        assert_eq!(verify_refusal(false, false, false, true), Some("Finish or cancel its download first."));
    }

    #[test]
    fn a_finished_update_or_repair_forgets_the_old_result() {
        let v = AppVerify::default();
        v.bad.lock().unwrap().insert("kelp".into(), FileCheck::default());
        v.bad.lock().unwrap().insert("tide".into(), FileCheck::default());
        v.forget("kelp");
        let bad = v.bad.lock().unwrap();
        assert!(!bad.contains_key("kelp"));
        assert!(bad.contains_key("tide"), "other games keep theirs");
    }

    #[test]
    fn delete_all_my_data_stops_any_running_check_and_waits_for_it() {
        let v = Arc::new(AppVerify::default());
        let stop = Arc::new(AtomicBool::new(false));
        *v.lock_current() = Some(("kelp".into(), stop.clone()));
        let (v2, s2) = (v.clone(), stop.clone());
        let checker = std::thread::spawn(move || {
            while !s2.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(5));
            }
            *v2.lock_current() = None;
        });
        assert!(tauri::async_runtime::block_on(v.stop_all(Duration::from_secs(5))));
        assert!(stop.load(Ordering::Relaxed));
        assert!(v.lock_current().is_none());
        checker.join().unwrap();
        assert!(tauri::async_runtime::block_on(AppVerify::default().stop_all(Duration::from_millis(10))), "nothing running is fine");
    }

    #[test]
    fn a_piece_isnt_removed_while_the_game_runs_or_is_checked() {
        assert_eq!(piece_refusal(true, true), Some("Close the game first."));
        assert_eq!(piece_refusal(false, true), Some("Wait for the file check to finish."));
        assert_eq!(piece_refusal(false, false), None);
    }

    #[test]
    fn repair_needs_the_installed_version() {
        assert_eq!(repair_needs_same_version("1.0", "1.0"), Ok(()));
        assert_eq!(repair_needs_same_version("1.0", "1.1"), Err("An update is waiting. Update the game, then verify again."));
    }
}
