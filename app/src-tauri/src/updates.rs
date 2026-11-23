//! Updates and settings commands (spec §6e).
use crate::downloads::DownloadsView;
use crate::{AppAuth, AppInstall, AppLibrary};
use fishy_core::install::manifest::parse_manifest;
use fishy_core::install::plan::build_update_plan;
use fishy_core::install::space::{check_space, free_space};
use fishy_core::install::parts::{only_parts, parts_for_update};
use fishy_core::install::store::{InstalledGame, JobInfo};
use fishy_core::install::InstallError;
use fishy_core::settings::{Settings, SettingsStore};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

const UPDATE_FETCH_ERROR: &str = "Couldn't get the update from Epic. Check your internet, then try again.";

pub struct AppSettings {
    pub store: SettingsStore,
}

/// Sign-in, format and space problems keep their own message; anything else while getting the
/// update is "Couldn't get the update".
fn update_error(e: InstallError) -> String {
    match e {
        InstallError::Rejected(_) | InstallError::Unsupported | InstallError::NoSpace { .. } | InstallError::Expired => e.user_message(),
        _ => UPDATE_FETCH_ERROR.to_string(),
    }
}

/// Queues an update of an installed game to Epic's newest version. Nothing happens if it's
/// already up to date or already in the Downloads list.
#[tauri::command]
pub async fn update_start(
    app: tauri::AppHandle,
    app_name: String,
    auth: tauri::State<'_, AppAuth>,
    library: tauri::State<'_, AppLibrary>,
    install: tauri::State<'_, AppInstall>,
    running: tauri::State<'_, crate::running::AppRunning>,
) -> Result<DownloadsView, String> {
    let result = queue_update(app.clone(), app_name.clone(), auth, library, install, running).await;
    if let Err(m) = &result {
        tracing::warn!("update of {app_name} refused: {m}");
        if m == crate::session::EXPIRED {
            crate::session::expire(&app).await;
        }
    }
    result
}

async fn queue_update(
    app: tauri::AppHandle,
    app_name: String,
    auth: tauri::State<'_, AppAuth>,
    library: tauri::State<'_, AppLibrary>,
    install: tauri::State<'_, AppInstall>,
    running: tauri::State<'_, crate::running::AppRunning>,
) -> Result<DownloadsView, String> {
    let downloads = &install.downloads;
    let all = downloads.store.installed();
    let installed = all.iter().find(|g| g.app_name == app_name).ok_or_else(|| InstallError::NotInstalled.user_message())?;
    if running.contains(&app_name) {
        return Err(fishy_core::running::close_first(&installed.title));
    }
    let cache = library.cache.load();
    let game = cache
        .as_ref()
        .and_then(|c| c.games.iter().find(|g| g.app_name == app_name).cloned())
        .ok_or_else(|| "Refresh your Library, then try again.".to_string())?;
    let latest = cache.map(|c| c.latest_versions).unwrap_or_default();
    // The game and its outdated pieces (spec §6n). Without Epic's version list, just the game:
    // its manifest tells whether there's anything new.
    let mut targets = pieces_to_update(&app_name, &all, &latest);
    if targets.is_empty() {
        targets.push(app_name.clone());
    }
    let token = auth.manager.access_token().await.map_err(|e| update_error(e.into()))?;
    let n = targets.len();
    for (i, target) in targets.iter().enumerate() {
        if downloads.has_job(target) {
            continue;
        }
        let Some(rec) = all.iter().find(|g| &g.app_name == target) else { continue };
        let (namespace, catalog_item_id) =
            if *target == app_name { (game.namespace.clone(), game.catalog_item_id.clone()) } else { (rec.namespace.clone(), rec.catalog_item_id.clone()) };
        if n > 1 {
            tracing::info!("update: {app_name} piece {} of {n}", i + 1);
        }
        queue_one(&app, downloads, rec, &namespace, &catalog_item_id, &token).await?;
    }
    Ok(downloads.view())
}

/// Queues one installed record (a game or a piece) to Epic's newest version, keeping its parts.
async fn queue_one(
    app: &tauri::AppHandle,
    downloads: &crate::downloads::Downloads,
    installed: &InstalledGame,
    namespace: &str,
    catalog_item_id: &str,
    token: &fishy_core::auth::Secret,
) -> Result<(), String> {
    let app_name = &installed.app_name;
    let info = downloads.client.manifest_info(token, namespace, catalog_item_id, app_name).await.map_err(update_error)?;
    let bytes = downloads.client.download_manifest(&info).await.map_err(update_error)?;
    let full = parse_manifest(&bytes).map_err(update_error)?;
    if full.meta.build_version == installed.version {
        return Ok(()); // Epic's list ran ahead of its manifests; nothing to do yet
    }
    let old_full = downloads.store.installed_manifest(app_name).ok().and_then(|b| parse_manifest(&b).ok());
    let parts = installed.parts.as_ref().map(|p| parts_for_update(p, old_full.as_ref(), &full));
    let manifest = only_parts(full, parts.as_deref());
    let old = old_full.map(|o| only_parts(o, installed.parts.as_deref()));
    let plan = build_update_plan(&manifest, old.as_ref(), &HashSet::new()).map_err(update_error)?;
    let path = installed.install_path.clone();
    let reserved = fishy_core::install::space::reserved_on_drive(&path, &downloads.left_to_write(app_name));
    check_space(&path, plan.space_needed + reserved, free_space(&path).unwrap_or(0)).map_err(update_error)?;
    let job = JobInfo {
        app_name: app_name.clone(),
        title: installed.title.clone(),
        namespace: namespace.to_string(),
        catalog_item_id: catalog_item_id.to_string(),
        install_path: path.clone(),
        download_size: plan.download_size,
        install_size: manifest.install_size(),
        base_urls: info.base_urls.clone(),
        update_from: Some(installed.version.clone()),
        imported: false,
        parts,
        parent: installed.parent.clone(),
    };
    downloads
        .store
        .save_job(&job, &bytes)
        .map_err(|e| InstallError::Io { folder: path.display().to_string(), detail: e.to_string() }.user_message())?;
    tracing::info!("update queued: {app_name} from {} ({} bytes to download)", installed.version, job.download_size);
    downloads.enqueue(app, job, Arc::new(manifest), old.map(Arc::new));
    Ok(())
}

/// What an Update of `game` downloads (spec §6n): the game if Epic has a newer version, then its
/// pieces (installed records whose parent is the game) that do, by app name.
pub(crate) fn pieces_to_update(game: &str, installed: &[InstalledGame], latest: &HashMap<String, String>) -> Vec<String> {
    let outdated = |g: &InstalledGame| latest.get(&g.app_name).is_some_and(|v| *v != g.version);
    let mut out: Vec<String> = installed.iter().filter(|g| g.app_name == game && outdated(g)).map(|g| g.app_name.clone()).collect();
    let mut pieces: Vec<String> = installed.iter().filter(|g| g.parent.as_deref() == Some(game) && outdated(g)).map(|g| g.app_name.clone()).collect();
    pieces.sort();
    out.extend(pieces);
    out
}

#[tauri::command]
pub fn settings_get(settings: tauri::State<'_, AppSettings>) -> Settings {
    settings.store.load()
}

#[tauri::command]
pub fn settings_set(value: Settings, settings: tauri::State<'_, AppSettings>, install: tauri::State<'_, AppInstall>) -> Result<Settings, String> {
    value.check().map_err(str::to_string)?;
    settings.store.save(&value).map_err(|_| {
        tracing::warn!("Couldn't save your settings.");
        "Couldn't save your settings.".to_string()
    })?;
    install.downloads.limiter.set(fishy_core::install::limit::limit_bytes(&value));
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(app: &str, version: &str, parent: Option<&str>) -> fishy_core::install::store::InstalledGame {
        fishy_core::install::store::InstalledGame {
            app_name: app.into(),
            title: app.into(),
            version: version.into(),
            install_path: "C:/Epic Games/Fortnite".into(),
            executable: String::new(),
            launch_parameters: String::new(),
            install_size: 1,
            installed_at: fishy_core::chrono::Utc::now(),
            last_played: None,
            parts: None,
            parent: parent.map(str::to_string),
            namespace: "ns".into(),
            catalog_item_id: "cat".into(),
        }
    }

    fn latest(pairs: &[(&str, &str)]) -> std::collections::HashMap<String, String> {
        pairs.iter().map(|(a, v)| (a.to_string(), v.to_string())).collect()
    }

    #[test]
    fn only_outdated_pieces_are_updated() {
        let ins = [rec("Fortnite", "43", None), rec("aa31", "42", Some("Fortnite")), rec("94bc", "43", Some("Fortnite"))];
        let got = pieces_to_update("Fortnite", &ins, &latest(&[("Fortnite", "43"), ("aa31", "43"), ("94bc", "43")]));
        assert_eq!(got, vec!["aa31".to_string()]);
    }

    #[test]
    fn game_first_then_pieces() {
        let ins = [rec("bb", "1", Some("Fortnite")), rec("Fortnite", "1", None), rec("aa", "1", Some("Fortnite"))];
        let got = pieces_to_update("Fortnite", &ins, &latest(&[("Fortnite", "2"), ("aa", "2"), ("bb", "2")]));
        assert_eq!(got, vec!["Fortnite".to_string(), "aa".into(), "bb".into()]);
    }

    #[test]
    fn pieces_of_other_games_are_ignored() {
        let ins = [rec("Sol", "1", None), rec("aa", "1", Some("Fortnite"))];
        let got = pieces_to_update("Sol", &ins, &latest(&[("Sol", "1"), ("aa", "2")]));
        assert!(got.is_empty());
    }

    #[test]
    fn update_fetch_error_says_what_to_do() {
        assert_eq!(UPDATE_FETCH_ERROR, "Couldn't get the update from Epic. Check your internet, then try again.");
    }

    #[test]
    fn update_errors_are_plain() {
        assert_eq!(update_error(InstallError::Network("x".into())), UPDATE_FETCH_ERROR);
        assert_eq!(update_error(InstallError::BadManifest("x".into())), UPDATE_FETCH_ERROR);
        assert_eq!(update_error(InstallError::NoDownload), UPDATE_FETCH_ERROR);
        assert_eq!(update_error(InstallError::Rejected("c".into())), "That sign-in didn't work. Please try again.");
        assert_eq!(update_error(InstallError::Unsupported), "This game uses a download format Fishy Client can't install yet.");
    }
}
