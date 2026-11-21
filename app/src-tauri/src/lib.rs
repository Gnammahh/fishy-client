mod downloads;
mod login_window;
mod play;

use fishy_core::auth::{parse_auth_code, AuthManager, AuthState, EpicClient, KeyringStore};
use downloads::{Downloads, DownloadsView};
use fishy_core::install::client::{InstallClient, ManifestInfo};
use fishy_core::install::manifest::{parse_manifest, Manifest};
use fishy_core::install::space::{check_space, default_install_root, folder_for, free_space};
use fishy_core::install::store::{InstallStore, InstalledGame, JobInfo};
use fishy_core::install::uninstall::uninstall;
use fishy_core::install::InstallError;
use fishy_core::library::{CachedLibrary, Game, LibraryCache, LibraryClient};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tokio::sync::Mutex;

/// Login state shared by all commands. `current` is None until startup has run once.
pub struct AppAuth {
    pub manager: AuthManager<KeyringStore>,
    current: Mutex<Option<AuthState>>,
}

impl AppAuth {
    pub async fn set(&self, state: AuthState) {
        *self.current.lock().await = Some(state);
    }
}

const NOT_A_CODE: &str = "That doesn't look like a sign-in code. Paste the authorizationCode value.";

/// Current login state. The first call checks the saved session (and refreshes it if needed).
#[tauri::command]
async fn auth_state(auth: tauri::State<'_, AppAuth>) -> Result<AuthState, String> {
    let mut current = auth.current.lock().await;
    if current.is_none() {
        *current = Some(auth.manager.startup().await);
    }
    Ok(current.clone().unwrap_or(AuthState::SignedOut))
}

/// Re-check the saved session (used by "Try again" when offline).
#[tauri::command]
async fn retry_auth(auth: tauri::State<'_, AppAuth>) -> Result<AuthState, String> {
    let state = auth.manager.startup().await;
    auth.set(state.clone()).await;
    Ok(state)
}

/// Must be async: creating a window from a sync command blocks the main thread on
/// Windows and leaves the new webview blank (tauri docs, "Create windows in async commands").
#[tauri::command]
async fn start_login(app: tauri::AppHandle) -> Result<(), String> {
    login_window::open(&app).map_err(|_| "Couldn't open the sign-in window. Try the paste-a-code option.".to_string())
}

/// Backup flow: the user pastes the code (or Epic's whole JSON) from their browser.
#[tauri::command]
async fn submit_code(input: String, auth: tauri::State<'_, AppAuth>) -> Result<AuthState, String> {
    let code = parse_auth_code(&input).ok_or_else(|| NOT_A_CODE.to_string())?;
    let state = auth.manager.complete_login(&code).await.map_err(|e| e.to_string())?;
    auth.set(state.clone()).await;
    Ok(state)
}

#[tauri::command]
async fn logout(
    app: tauri::AppHandle,
    auth: tauri::State<'_, AppAuth>,
    library: tauri::State<'_, AppLibrary>,
    install: tauri::State<'_, AppInstall>,
) -> Result<AuthState, String> {
    // Downloads pause; installed games and their records stay (spec §6c).
    install.downloads.pause_all(&app);
    install.prepared.lock().unwrap_or_else(|e| e.into_inner()).clear();
    let state = auth.manager.logout().await;
    let _ = library.cache.clear(); // the saved game list goes too (spec §6b)
    auth.set(state.clone()).await;
    Ok(state)
}

/// Library access: Epic client plus the on-disk cache.
pub struct AppLibrary {
    client: LibraryClient,
    cache: LibraryCache,
}

const LIBRARY_ERROR: &str = "Couldn't load your library from Epic.";

/// The last saved library (instant, no network), if any.
#[tauri::command]
fn library_cached(library: tauri::State<'_, AppLibrary>) -> Option<CachedLibrary> {
    library.cache.load()
}

/// Fetches the library from Epic, saves it, and returns it.
#[tauri::command]
async fn library_refresh(auth: tauri::State<'_, AppAuth>, library: tauri::State<'_, AppLibrary>) -> Result<CachedLibrary, String> {
    let token = auth.manager.access_token().await.map_err(|_| LIBRARY_ERROR.to_string())?;
    let games = library.client.fetch_library(&token).await.map_err(|_| LIBRARY_ERROR.to_string())?;
    let fresh = CachedLibrary { updated_at: fishy_core::chrono::Utc::now(), games };
    let _ = library.cache.save(&fresh); // a failed save only costs the instant start next time
    Ok(fresh)
}

/// Installs: the downloads manager plus manifests fetched for the Install box.
pub struct AppInstall {
    downloads: Downloads,
    prepared: std::sync::Mutex<HashMap<String, Prepared>>,
}

/// What the Install box fetched, reused by install_start so the manifest isn't downloaded twice.
#[derive(Clone)]
struct Prepared {
    info: ManifestInfo,
    bytes: Arc<Vec<u8>>,
    manifest: Arc<Manifest>,
    folder: Option<String>,
}

#[derive(Serialize)]
struct InstallOptions {
    install_path: String,
    download_size: u64,
    install_size: u64,
    free_space: u64,
    enough_space: bool,
    error: Option<String>,
}

async fn prepare(game: &Game, auth: &AppAuth, library: &AppLibrary, install: &AppInstall) -> Result<Prepared, InstallError> {
    if let Some(p) = install.prepared.lock().unwrap_or_else(|e| e.into_inner()).get(&game.app_name).cloned() {
        return Ok(p);
    }
    let token = auth.manager.access_token().await?;
    let client = &install.downloads.client;
    let info = client.manifest_info(&token, &game.namespace, &game.catalog_item_id, &game.app_name).await?;
    let bytes = client.download_manifest(&info).await?;
    let manifest = parse_manifest(&bytes)?;
    let folder = library.client.catalog_item(&token, &game.namespace, &game.catalog_item_id).await.ok().flatten().and_then(|c| c.folder_name());
    let p = Prepared { info, bytes: Arc::new(bytes), manifest: Arc::new(manifest), folder };
    install.prepared.lock().unwrap_or_else(|e| e.into_inner()).insert(game.app_name.clone(), p.clone());
    Ok(p)
}

fn install_path(game: &Game, parent: Option<String>, p: &Prepared) -> PathBuf {
    let root = parent.filter(|s| !s.trim().is_empty()).map(PathBuf::from).unwrap_or_else(default_install_root);
    folder_for(&root, p.folder.as_deref(), &game.title)
}

/// Everything the Install box shows: folder, sizes and free space.
#[tauri::command]
async fn install_options(
    game: Game,
    parent: Option<String>,
    auth: tauri::State<'_, AppAuth>,
    library: tauri::State<'_, AppLibrary>,
    install: tauri::State<'_, AppInstall>,
) -> Result<InstallOptions, String> {
    let p = prepare(&game, &auth, &library, &install).await.map_err(|e| e.user_message())?;
    let path = install_path(&game, parent, &p);
    let install_size = p.manifest.install_size();
    let free = free_space(&path).unwrap_or(0);
    let space = check_space(&path, install_size, free);
    Ok(InstallOptions {
        install_path: path.display().to_string(),
        download_size: p.manifest.chunks.iter().map(|c| c.file_size).sum(),
        install_size,
        free_space: free,
        enough_space: space.is_ok(),
        error: space.err().map(|e| e.user_message()),
    })
}

#[tauri::command]
async fn install_start(
    app: tauri::AppHandle,
    game: Game,
    parent: Option<String>,
    auth: tauri::State<'_, AppAuth>,
    library: tauri::State<'_, AppLibrary>,
    install: tauri::State<'_, AppInstall>,
) -> Result<DownloadsView, String> {
    let downloads = &install.downloads;
    if downloads.has_job(&game.app_name) || downloads.store.installed().iter().any(|g| g.app_name == game.app_name) {
        return Ok(downloads.view());
    }
    let p = prepare(&game, &auth, &library, &install).await.map_err(|e| e.user_message())?;
    let path = install_path(&game, parent, &p);
    let install_size = p.manifest.install_size();
    check_space(&path, install_size, free_space(&path).unwrap_or(0)).map_err(|e| e.user_message())?;
    let job = JobInfo {
        app_name: game.app_name.clone(),
        title: game.title.clone(),
        namespace: game.namespace.clone(),
        catalog_item_id: game.catalog_item_id.clone(),
        install_path: path.clone(),
        download_size: p.manifest.chunks.iter().map(|c| c.file_size).sum(),
        install_size,
        base_urls: p.info.base_urls.clone(),
        update_from: None,
    };
    downloads
        .store
        .save_job(&job, &p.bytes)
        .map_err(|e| InstallError::Io { folder: path.display().to_string(), detail: e.to_string() }.user_message())?;
    install.prepared.lock().unwrap_or_else(|e| e.into_inner()).remove(&game.app_name);
    downloads.enqueue(&app, job, p.manifest);
    Ok(downloads.view())
}

/// The system folder picker. Async: dialogs must not block the main thread.
#[tauri::command]
async fn pick_folder(app: tauri::AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().set_title("Choose where to install games").pick_folder(move |picked| {
        let _ = tx.send(picked);
    });
    rx.await.ok().flatten().and_then(|p| p.into_path().ok()).map(|p| p.display().to_string())
}

#[tauri::command]
fn downloads_state(install: tauri::State<'_, AppInstall>) -> DownloadsView {
    install.downloads.view()
}

#[tauri::command]
fn download_pause(app: tauri::AppHandle, app_name: String, install: tauri::State<'_, AppInstall>) {
    install.downloads.pause(&app, &app_name);
}

#[tauri::command]
fn download_resume(app: tauri::AppHandle, app_name: String, install: tauri::State<'_, AppInstall>) {
    install.downloads.resume(&app, &app_name);
}

#[tauri::command]
async fn download_cancel(app: tauri::AppHandle, app_name: String, install: tauri::State<'_, AppInstall>) -> Result<(), String> {
    install.downloads.cancel(&app, &app_name).await
}

#[tauri::command]
fn installed_list(install: tauri::State<'_, AppInstall>) -> Vec<InstalledGame> {
    install.downloads.store.installed()
}

#[tauri::command]
async fn uninstall_game(app: tauri::AppHandle, app_name: String, install: tauri::State<'_, AppInstall>) -> Result<Vec<InstalledGame>, String> {
    let store = install.downloads.store.clone();
    let name = app_name.clone();
    tauri::async_runtime::spawn_blocking(move || uninstall(&store, &name))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.user_message())?;
    let _ = app.emit("installed-changed", ());
    Ok(install.downloads.store.installed())
}

#[tauri::command]
fn open_login_in_browser() -> Result<(), String> {
    login_window::open_in_browser().map_err(|_| "Couldn't open your browser.".to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            app.manage(AppAuth {
                manager: AuthManager::new(EpicClient::new(), KeyringStore::windows(), chrono_now),
                current: Mutex::new(None),
            });
            app.manage(AppLibrary { client: LibraryClient::new(), cache: LibraryCache::default_location() });
            app.manage(AppInstall {
                downloads: Downloads::new(InstallStore::default_location(), InstallClient::new()),
                prepared: std::sync::Mutex::new(HashMap::new()),
            });
            app.manage(play::AppLaunch::new());
            Ok(())
        })
        // Closing the main window quits the app, even if the sign-in window is still open.
        .on_window_event(|window, event| {
            if window.label() == "main" && matches!(event, tauri::WindowEvent::Destroyed) {
                window.app_handle().exit(0);
            }
        })
        .invoke_handler(tauri::generate_handler![
            auth_state,
            retry_auth,
            start_login,
            submit_code,
            logout,
            open_login_in_browser,
            library_cached,
            library_refresh,
            install_options,
            install_start,
            pick_folder,
            downloads_state,
            download_pause,
            download_resume,
            download_cancel,
            installed_list,
            uninstall_game,
            play::play_game
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn chrono_now() -> fishy_core::chrono::DateTime<fishy_core::chrono::Utc> {
    fishy_core::chrono::Utc::now()
}
