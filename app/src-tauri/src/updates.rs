//! Updates and settings commands (spec §6e).
use crate::downloads::DownloadsView;
use crate::{AppAuth, AppInstall, AppLibrary};
use fishy_core::install::manifest::parse_manifest;
use fishy_core::install::plan::build_update_plan;
use fishy_core::install::space::{check_space, free_space};
use fishy_core::install::store::JobInfo;
use fishy_core::install::InstallError;
use fishy_core::settings::{Settings, SettingsStore};
use std::collections::HashSet;
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
    if downloads.has_job(&app_name) {
        return Ok(downloads.view());
    }
    let installed = downloads.store.installed().into_iter().find(|g| g.app_name == app_name).ok_or_else(|| InstallError::NotInstalled.user_message())?;
    if running.contains(&app_name) {
        return Err(fishy_core::running::close_first(&installed.title));
    }
    let game = library
        .cache
        .load()
        .and_then(|c| c.games.into_iter().find(|g| g.app_name == app_name))
        .ok_or_else(|| "Refresh your Library, then try again.".to_string())?;

    let token = auth.manager.access_token().await.map_err(|e| update_error(e.into()))?;
    let info = downloads.client.manifest_info(&token, &game.namespace, &game.catalog_item_id, &app_name).await.map_err(update_error)?;
    let bytes = downloads.client.download_manifest(&info).await.map_err(update_error)?;
    let manifest = parse_manifest(&bytes).map_err(update_error)?;
    if manifest.meta.build_version == installed.version {
        return Ok(downloads.view()); // Epic's list ran ahead of its manifests; nothing to do yet
    }
    let old = downloads.store.installed_manifest(&app_name).ok().and_then(|b| parse_manifest(&b).ok());
    let plan = build_update_plan(&manifest, old.as_ref(), &HashSet::new()).map_err(update_error)?;
    let path = installed.install_path.clone();
    let reserved = fishy_core::install::space::reserved_on_drive(&path, &downloads.left_to_write(&app_name));
    check_space(&path, plan.space_needed + reserved, free_space(&path).unwrap_or(0)).map_err(update_error)?;
    let job = JobInfo {
        app_name: app_name.clone(),
        title: installed.title.clone(),
        namespace: game.namespace.clone(),
        catalog_item_id: game.catalog_item_id.clone(),
        install_path: path.clone(),
        download_size: plan.download_size,
        install_size: manifest.install_size(),
        base_urls: info.base_urls.clone(),
        update_from: Some(installed.version.clone()),
        imported: false,
        parts: None,
        parent: None,
    };
    downloads
        .store
        .save_job(&job, &bytes)
        .map_err(|e| InstallError::Io { folder: path.display().to_string(), detail: e.to_string() }.user_message())?;
    tracing::info!("update queued: {app_name} from {} ({} bytes to download)", installed.version, job.download_size);
    downloads.enqueue(&app, job, Arc::new(manifest), old.map(Arc::new));
    Ok(downloads.view())
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
