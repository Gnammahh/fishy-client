//! Adding games already installed by Epic's own launcher (spec §6g).
use crate::downloads::DownloadsView;
use crate::{AppAuth, AppInstall, AppLibrary};
use fishy_core::epic_import::{self as epic, EpicItem};
use fishy_core::install::client::ManifestInfo;
use fishy_core::install::manifest::Manifest;
use fishy_core::install::store::InstalledGame;
use fishy_core::install::InstallError;
use fishy_core::library::Game;
use serde::Serialize;
use std::sync::Arc;
use tracing::{info, warn};

const REFRESH: &str = "Refresh your Library, then try again.";

fn unreadable(title: &str) -> String {
    format!("Couldn't read {title}'s files from Epic's launcher. Press Install to download it.")
}

fn offline(title: &str) -> String {
    format!("Couldn't reach Epic to check {title}. Check your internet.")
}

fn partial(title: &str) -> String {
    format!("{title} was installed with only some of its parts, which Fishy Client can't add yet. Keep using Epic's launcher for it.")
}

/// A game the Library note offers to add.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Candidate {
    pub app_name: String,
    pub title: String,
}

/// What Add did for one game.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum AddOutcome {
    Added,
    Missing { files: usize, bytes: u64 },
    Error { message: String, signed_out: bool },
}

/// Epic's games that are in the Library, not installed here and not in the Downloads list.
fn candidates(items: Vec<EpicItem>, games: &[Game], installed: &[InstalledGame], has_job: impl Fn(&str) -> bool) -> Vec<(EpicItem, Game)> {
    items
        .into_iter()
        .filter(|i| !installed.iter().any(|g| g.app_name == i.app_name) && !has_job(&i.app_name))
        .filter_map(|i| games.iter().find(|g| g.app_name == i.app_name).cloned().map(|g| (i, g)))
        .collect()
}

fn current(library: &AppLibrary, install: &AppInstall) -> Vec<(EpicItem, Game)> {
    let Some(dir) = epic::epic_manifests_dir() else { return Vec::new() };
    let games = library.cache.load().map(|c| c.games).unwrap_or_default();
    let downloads = &install.downloads;
    candidates(epic::scan(&dir), &games, &downloads.store.installed(), |app| downloads.has_job(app))
}

fn find(app_name: &str, library: &AppLibrary, install: &AppInstall) -> Option<(EpicItem, Game)> {
    current(library, install).into_iter().find(|(i, _)| i.app_name == app_name)
}

/// The manifest: the game's own `.egstore` copy, else Epic's current one. With `need_info`, the
/// manifest info is fetched too (its CDN addresses are needed to download anything).
async fn manifest_for(item: &EpicItem, game: &Game, auth: &AppAuth, install: &AppInstall, need_info: bool) -> Result<(Vec<u8>, Manifest, Option<ManifestInfo>), InstallError> {
    let local = epic::local_manifest(item).and_then(|b| epic::usable_manifest(item, &b).map(|m| (b, m)));
    if let (Some((bytes, m)), false) = (&local, need_info) {
        return Ok((bytes.clone(), m.clone(), None));
    }
    let client = &install.downloads.client;
    let token = auth.manager.access_token().await?;
    let info = client.manifest_info(&token, &game.namespace, &game.catalog_item_id, &game.app_name).await?;
    if let Some((bytes, m)) = local {
        return Ok((bytes, m, Some(info)));
    }
    info!("epic import: {} has no usable file list, downloading Epic's", game.app_name);
    let bytes = client.download_manifest(&info).await?;
    let m = epic::usable_manifest(item, &bytes).ok_or(InstallError::Unsupported)?;
    Ok((bytes, m, Some(info)))
}

/// The user-facing message for a failure, and whether the sign-in ended.
async fn failure(app: &tauri::AppHandle, title: &str, e: InstallError) -> (String, bool) {
    match e {
        InstallError::Expired => {
            crate::session::expire(app).await;
            (crate::session::EXPIRED.to_string(), true)
        }
        InstallError::Network(_) | InstallError::Rejected(_) => (offline(title), false),
        InstallError::Unsupported | InstallError::BadManifest(_) | InstallError::NoDownload => (unreadable(title), false),
        other => (other.user_message(), false),
    }
}

async fn check(item: &EpicItem, m: &Manifest) -> epic::FileCheck {
    let (root, m) = (item.install_path.clone(), m.clone());
    tokio::task::spawn_blocking(move || epic::check_files(&root, &m)).await.unwrap_or_else(|_| epic::FileCheck { bad_files: 1, ..Default::default() })
}

#[tauri::command]
pub fn epic_import_scan(library: tauri::State<'_, AppLibrary>, install: tauri::State<'_, AppInstall>) -> Vec<Candidate> {
    let found: Vec<Candidate> = current(&library, &install).into_iter().map(|(_, g)| Candidate { app_name: g.app_name, title: g.title }).collect();
    info!("epic import: {} found", found.len());
    found
}

/// Checks one game and adds it if every file is there.
#[tauri::command]
pub async fn epic_import_add(
    app: tauri::AppHandle,
    app_name: String,
    auth: tauri::State<'_, AppAuth>,
    library: tauri::State<'_, AppLibrary>,
    install: tauri::State<'_, AppInstall>,
) -> Result<AddOutcome, String> {
    let error = |message: String, signed_out: bool| Ok(AddOutcome::Error { message, signed_out });
    let Some((item, game)) = find(&app_name, &library, &install) else { return error(REFRESH.into(), false) };
    if item.partial {
        warn!("epic import: {app_name} has optional parts, not added");
        return error(partial(&game.title), false);
    }
    let (bytes, m, _) = match manifest_for(&item, &game, &auth, &install, false).await {
        Ok(x) => x,
        Err(e) => {
            let (message, signed_out) = failure(&app, &game.title, e).await;
            return error(message, signed_out);
        }
    };
    let result = check(&item, &m).await;
    if !result.all_good() {
        info!("epic import: {app_name} has {} files to download", result.bad_files);
        return Ok(AddOutcome::Missing { files: result.bad_files, bytes: result.bad_bytes });
    }
    let store = &install.downloads.store;
    let job = epic::import_job(&item, &game.title, &m, Vec::new());
    match epic::add_installed(store, &job, &bytes, &m) {
        Ok(_) => {
            info!("epic import: added {app_name}");
            Ok(AddOutcome::Added)
        }
        Err(e) => {
            let _ = store.remove_job(&app_name);
            warn!("epic import: couldn't save {app_name}: {}", e.kind());
            error(unreadable(&game.title), false)
        }
    }
}

/// "Download them": queues a download of only the missing or damaged files.
#[tauri::command]
pub async fn epic_import_repair(
    app: tauri::AppHandle,
    app_name: String,
    auth: tauri::State<'_, AppAuth>,
    library: tauri::State<'_, AppLibrary>,
    install: tauri::State<'_, AppInstall>,
) -> Result<DownloadsView, String> {
    let downloads = &install.downloads;
    let Some((item, game)) = find(&app_name, &library, &install) else { return Err(REFRESH.into()) };
    if item.partial {
        return Err(partial(&game.title));
    }
    let (bytes, m, info) = match manifest_for(&item, &game, &auth, &install, true).await {
        Ok(x) => x,
        Err(e) => return Err(failure(&app, &game.title, e).await.0),
    };
    let base_urls = info.map(|i| i.base_urls).unwrap_or_default();
    let result = check(&item, &m).await;
    let job = epic::import_job(&item, &game.title, &m, base_urls);
    if let Err(e) = epic::save_repair(&downloads.store, &job, &bytes, &result) {
        let _ = downloads.store.remove_job(&app_name);
        warn!("epic import: couldn't save the repair of {app_name}: {}", e.kind());
        return Err(unreadable(&game.title));
    }
    info!("epic import: repair of {app_name} queued ({} files)", result.bad_files);
    downloads.enqueue(&app, job, Arc::new(m), None);
    Ok(downloads.view())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn item(app: &str) -> EpicItem {
        EpicItem {
            app_name: app.into(),
            title: format!("Epic {app}"),
            install_path: PathBuf::from("C:/Epic Games").join(app),
            installation_guid: "G".into(),
            version: "1".into(),
            namespace: "ns".into(),
            catalog_item_id: "cat".into(),
            partial: false,
        }
    }

    fn game(app: &str) -> Game {
        Game {
            app_name: app.into(),
            namespace: "ns".into(),
            catalog_item_id: "cat".into(),
            title: format!("Library {app}"),
            cover_url: None,
            installable: true,
            launch: Default::default(),
        }
    }

    fn record(app: &str) -> InstalledGame {
        InstalledGame {
            app_name: app.into(),
            title: app.into(),
            version: "1".into(),
            install_path: PathBuf::from("C:/x"),
            executable: "x.exe".into(),
            launch_parameters: String::new(),
            install_size: 1,
            installed_at: fishy_core::chrono::Utc::now(),
            last_played: None,
        }
    }

    #[test]
    fn candidates_keep_only_library_games() {
        let got = candidates(vec![item("A"), item("NotMine")], &[game("A"), game("B")], &[], |_| false);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0.app_name, "A");
        assert_eq!(got[0].1.title, "Library A");
    }

    #[test]
    fn candidates_skip_installed_and_queued() {
        let got = candidates(vec![item("A"), item("B"), item("C")], &[game("A"), game("B"), game("C")], &[record("A")], |app| app == "B");
        let names: Vec<&str> = got.iter().map(|(i, _)| i.app_name.as_str()).collect();
        assert_eq!(names, vec!["C"]);
    }
}
