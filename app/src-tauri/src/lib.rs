mod app_update;
mod data;
mod downloads;
mod epic_import;
mod login_window;
mod play;
mod running;
mod session;
mod store_window;
mod updates;

use fishy_core::auth::{parse_auth_code, AuthManager, AuthState, EpicClient, KeyringStore};
use downloads::{Downloads, DownloadsView};
use fishy_core::install::client::{InstallClient, ManifestInfo};
use fishy_core::install::manifest::{parse_manifest, Manifest};
use fishy_core::install::space::{check_space, folder_for, free_space, install_root, reserved_on_drive, unique_folder};
use fishy_core::install::store::{InstallStore, InstalledGame, JobInfo};
use fishy_core::install::uninstall::uninstall;
use fishy_core::install::InstallError;
use fishy_core::achievements::{Achievements, AchievementsClient};
use fishy_core::library::{CachedLibrary, Game, LibraryCache, LibraryClient};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tokio::sync::Mutex;
use tracing::{info, warn};

/// Only the kind of state goes to the log, never the name or account ID.
pub(crate) fn state_name(s: &AuthState) -> &'static str {
    match s {
        AuthState::SignedOut => "signed out",
        AuthState::SignedIn { .. } => "signed in",
        AuthState::Offline { .. } => "offline",
    }
}

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
        let state = auth.manager.startup().await;
        info!("startup sign-in check: {}", state_name(&state));
        *current = Some(state);
    }
    Ok(current.clone().unwrap_or(AuthState::SignedOut))
}

/// Re-check the saved session (used by "Try again" when offline).
#[tauri::command]
async fn retry_auth(auth: tauri::State<'_, AppAuth>) -> Result<AuthState, String> {
    let state = auth.manager.startup().await;
    info!("sign-in re-check: {}", state_name(&state));
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
    let state = auth.manager.complete_login(&code).await.map_err(|e| {
        warn!("sign-in with a pasted code failed"); // Epic's text can quote secrets, so no details
        e.to_string()
    })?;
    info!("signed in with a pasted code");
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
    store_window::sign_out(&app); // and the Store window's sign-in (spec §6o)
    info!("logged out");
    auth.set(state.clone()).await;
    Ok(state)
}

/// Library access: Epic client plus the on-disk cache.
pub struct AppLibrary {
    client: LibraryClient,
    cache: LibraryCache,
    achievements: AchievementsClient,
}

impl AppLibrary {
    /// The saved Library's games, for "In your Library" in the Store (spec §6o).
    pub fn owned(&self) -> fishy_core::store::Owned {
        let games = self.cache.load().map(|c| c.games).unwrap_or_default();
        fishy_core::store::Owned {
            catalog_item_ids: games.iter().map(|g| g.catalog_item_id.clone()).collect(),
            namespaces: games.into_iter().map(|g| g.namespace).collect(),
        }
    }
}

const LIBRARY_ERROR: &str = "Couldn't load your library from Epic. Check your internet, then press Try again.";
const BROWSER_ERROR: &str = "Couldn't open your browser. Use Sign in with Epic instead.";

/// The last saved library (instant, no network), if any.
#[tauri::command]
fn library_cached(library: tauri::State<'_, AppLibrary>) -> Option<CachedLibrary> {
    library.cache.load()
}

/// A refresh may only be saved if the same account is still signed in when it finishes: the
/// sign-in can expire, or the user log out, while the games are being fetched.
fn may_save_refresh(before: Option<&AuthState>, now: Option<&AuthState>) -> bool {
    match (before, now) {
        (Some(AuthState::SignedIn { account_id: was, .. }), Some(AuthState::SignedIn { account_id: is, .. })) => was == is,
        (_, Some(AuthState::SignedIn { .. })) => true,
        _ => false,
    }
}

/// Fetches the library from Epic, saves it, and returns it.
#[tauri::command]
async fn library_refresh(app: tauri::AppHandle, auth: tauri::State<'_, AppAuth>, library: tauri::State<'_, AppLibrary>) -> Result<CachedLibrary, String> {
    let started = std::time::Instant::now();
    let failed = || {
        warn!("library refresh failed: {LIBRARY_ERROR}");
        LIBRARY_ERROR.to_string()
    };
    let before = auth.current.lock().await.clone();
    let token = match auth.manager.access_token().await {
        Ok(t) => t,
        Err(fishy_core::auth::AuthError::Expired) => {
            session::expire(&app).await;
            return Err(session::EXPIRED.to_string());
        }
        Err(_) => return Err(failed()),
    };
    let data = library.client.fetch_library(&token).await.map_err(|_| failed())?;
    info!("library refreshed: {} games in {:.1} s", data.games.len(), started.elapsed().as_secs_f64());
    if !may_save_refresh(before.as_ref(), auth.current.lock().await.as_ref()) {
        warn!("library refresh dropped: the sign-in changed while it ran");
        return Err(failed());
    }
    let fresh = CachedLibrary { updated_at: fishy_core::chrono::Utc::now(), games: data.games, latest_versions: data.latest_versions };
    let _ = library.cache.save(&fresh); // a failed save only costs the instant start next time
    Ok(fresh)
}

/// What the game page shows when achievements can't be loaded (spec §6q).
const ACHIEVEMENTS_ERROR: &str = "Couldn't load achievements from Epic.";

/// The game's namespace, from the saved library only (never from the page).
fn namespace_of(games: &[Game], app_name: &str) -> Option<String> {
    games.iter().find(|g| g.app_name == app_name).map(|g| g.namespace.clone())
}

/// A game's achievements and which ones the player unlocked (spec §6q).
#[tauri::command]
async fn game_achievements(app: tauri::AppHandle, app_name: String, auth: tauri::State<'_, AppAuth>, library: tauri::State<'_, AppLibrary>) -> Result<Achievements, String> {
    let failed = || {
        warn!("achievements failed");
        ACHIEVEMENTS_ERROR.to_string()
    };
    let games = library.cache.load().map(|c| c.games).unwrap_or_default();
    let namespace = namespace_of(&games, &app_name).ok_or_else(failed)?;
    let token = match auth.manager.access_token().await {
        Ok(t) => t,
        Err(fishy_core::auth::AuthError::Expired) => {
            session::expire(&app).await;
            return Err(session::EXPIRED.to_string());
        }
        Err(_) => return Err(failed()),
    };
    let (_, account_id) = auth.manager.saved_identity().ok_or_else(failed)?;
    let a = library.achievements.fetch(&token, &account_id, &namespace).await.map_err(|_| failed())?;
    info!("achievements loaded: {} of {}", a.unlocked, a.total);
    Ok(a)
}

/// Installs: the downloads manager plus manifests fetched for the Install box.
pub struct AppInstall {
    downloads: Downloads,
    prepared: std::sync::Mutex<HashMap<String, Prepared>>,
    /// Games whose files are being checked for an import (spec §6g); Install waits for them.
    checking: epic_import::Checking,
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
    /// False when Windows won't let Fishy Client create the folder (spec §6g.3).
    folder_ok: bool,
    /// Windows will ask for permission once to create the folder (spec §6k).
    needs_permission: bool,
    error: Option<String>,
    note: Option<String>,
    /// The game has optional parts: it's installed with Epic's launcher, then imported (spec §6n).
    has_parts: bool,
}

/// Why Install is refused for a game with optional parts (spec §6n).
const HAS_PARTS: &str = "This game has optional parts. Install it with Epic's launcher; Fishy Client adds it by itself afterwards.";

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

/// The parent picked in the Install box, else the saved default folder (spec §6f.1). The note
/// is set when the saved default couldn't be used.
fn install_path(game: &Game, parent: Option<String>, p: &Prepared, settings: &updates::AppSettings, downloads: &Downloads) -> (PathBuf, Option<String>) {
    let (root, note) = match parent.filter(|s| !s.trim().is_empty()) {
        Some(picked) => (PathBuf::from(picked), None),
        None => {
            let (root, unavailable) = install_root(settings.store.load().install_folder.as_deref());
            let note = unavailable.then(|| data::unavailable_folder_note(&root));
            (root, note)
        }
    };
    // Two games never share a folder: the second gets "Name (2)".
    (unique_folder(folder_for(&root, p.folder.as_deref(), &game.title), &downloads.taken_folders(&game.app_name)), note)
}

/// The admin copy may create this game folder (spec §6k path rules). Touches the disk, so it
/// runs off the async thread.
async fn permission_possible(path: &std::path::Path) -> bool {
    let p = path.to_path_buf();
    tokio::task::spawn_blocking(move || fishy_core::install::elevate::check_new_folder(&p, &fishy_core::install::elevate::windows_dir()).is_ok())
        .await
        .unwrap_or(false)
}

/// After Windows' permission prompt (which can stay open for minutes): nothing else installed,
/// queued, imported or wiped the game meanwhile.
fn still_queueable(installed: bool, has_job: bool, being_imported: bool, wiping: bool) -> bool {
    !(installed || has_job || being_imported || wiping)
}

/// The Install box's folder fields: (folder_ok, needs_permission, error). A folder Fishy Client
/// can't create itself asks Windows for permission when that's safe, else says why not.
fn folder_fields(res: Result<(), PathBuf>, safe: bool) -> (bool, bool, Option<String>) {
    match res {
        Ok(()) => (true, false, None),
        Err(_) if safe => (false, true, None),
        Err(folder) => (false, false, Some(InstallError::NoWriteAccess { folder: folder.display().to_string() }.user_message())),
    }
}

/// After Windows' permission: the folder must now be writable, or nothing is queued.
fn after_permission(writable: bool, folder: &str) -> Result<(), InstallError> {
    if writable {
        Ok(())
    } else {
        Err(InstallError::FolderFailed { folder: folder.to_string() })
    }
}

/// Makes sure the game folder can be written, asking Windows for permission once if needed.
async fn ensure_folder(path: &std::path::Path) -> Result<(), InstallError> {
    let p = path.to_path_buf();
    let res = tokio::task::spawn_blocking(move || fishy_core::install::space::can_create_in(&p)).await.unwrap_or(Ok(()));
    let Err(folder) = res else { return Ok(()) };
    if !permission_possible(path).await {
        return Err(InstallError::NoWriteAccess { folder: folder.display().to_string() });
    }
    info!("install: asking for permission to create a game folder");
    let p = path.to_path_buf();
    #[cfg(windows)]
    let asked = tokio::task::spawn_blocking(move || fishy_core::install::elevate::ask_for_folder(&p)).await.unwrap_or(fishy_core::install::elevate::Asked::Failed);
    #[cfg(not(windows))]
    let asked = { let _ = p; fishy_core::install::elevate::Asked::Failed };
    let shown = path.display().to_string();
    match asked {
        fishy_core::install::elevate::Asked::Created => {
            let p = path.to_path_buf();
            let writable = tokio::task::spawn_blocking(move || fishy_core::install::space::can_create_in(&p.join("probe")).is_ok()).await.unwrap_or(false);
            if writable {
                info!("install: game folder created with permission");
            }
            after_permission(writable, &shown)
        }
        fishy_core::install::elevate::Asked::Declined => {
            info!("install: permission declined");
            Err(InstallError::PermissionDeclined)
        }
        fishy_core::install::elevate::Asked::Failed => Err(InstallError::FolderFailed { folder: shown }),
    }
}

/// `%ProgramFiles%\Epic Games`, when Epic's launcher has made it (spec §6k).
#[tauri::command]
fn epic_folder() -> Option<String> {
    let base = std::env::var_os("ProgramFiles")?;
    let dir = PathBuf::from(base).join("Epic Games");
    dir.is_dir().then(|| dir.display().to_string())
}

/// Everything the Install box shows: folder, sizes and free space.
#[tauri::command]
async fn install_options(
    app: tauri::AppHandle,
    game: Game,
    parent: Option<String>,
    auth: tauri::State<'_, AppAuth>,
    library: tauri::State<'_, AppLibrary>,
    install: tauri::State<'_, AppInstall>,
    settings: tauri::State<'_, updates::AppSettings>,
) -> Result<InstallOptions, String> {
    let p = match prepare(&game, &auth, &library, &install).await {
        Ok(p) => p,
        Err(e) => {
            if session::is_expired_install(&e) {
                session::expire(&app).await;
            }
            return Err(e.user_message());
        }
    };
    let (path, note) = install_path(&game, parent, &p, &settings, &install.downloads);
    let install_size = p.manifest.install_size();
    let free = free_space(&path).unwrap_or(0);
    // Games already waiting in the queue on the same drive need their space too.
    let reserved = reserved_on_drive(&path, &install.downloads.left_to_write(&game.app_name));
    let space = check_space(&path, install_size + reserved, free);
    let check_path = path.clone();
    let res = tokio::task::spawn_blocking(move || fishy_core::install::space::can_create_in(&check_path)).await.unwrap_or(Ok(()));
    let (folder_ok, needs_permission, folder_error) = folder_fields(res, permission_possible(&path).await);
    Ok(InstallOptions {
        install_path: path.display().to_string(),
        download_size: p.manifest.chunks.iter().map(|c| c.file_size).sum(),
        install_size,
        free_space: free,
        enough_space: space.is_ok(),
        folder_ok: folder_ok || needs_permission,
        needs_permission,
        error: folder_error.or(space.err().map(|e| e.user_message())),
        note,
        has_parts: fishy_core::install::parts::has_parts(&p.manifest),
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
    settings: tauri::State<'_, updates::AppSettings>,
) -> Result<DownloadsView, String> {
    let downloads = &install.downloads;
    if downloads.has_job(&game.app_name) || downloads.store.installed().iter().any(|g| g.app_name == game.app_name) {
        return Ok(downloads.view());
    }
    if install.checking.is_busy(&game.app_name) {
        return Err(epic_import::being_checked(&game.title));
    }
    let refused = |e: InstallError| {
        let m = e.user_message();
        warn!("install of {} refused: {m}", game.app_name);
        m
    };
    let p = match prepare(&game, &auth, &library, &install).await {
        Ok(p) => p,
        Err(e) => {
            if session::is_expired_install(&e) {
                session::expire(&app).await;
            }
            return Err(refused(e));
        }
    };
    if fishy_core::install::parts::has_parts(&p.manifest) {
        return Err(HAS_PARTS.to_string());
    }
    let (path, _) = install_path(&game, parent, &p, &settings, downloads);
    let install_size = p.manifest.install_size();
    let reserved = reserved_on_drive(&path, &downloads.left_to_write(&game.app_name));
    check_space(&path, install_size + reserved, free_space(&path).unwrap_or(0)).map_err(refused)?;
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
        imported: false,
        parts: None,
        parent: None,
    };
    // An import may have started while the manifest was fetched.
    if install.checking.is_busy(&game.app_name) || downloads.has_job(&game.app_name) {
        return Err(epic_import::being_checked(&game.title));
    }
    // Last, so Windows' permission prompt only appears for an install that will really start.
    ensure_folder(&path).await.map_err(refused)?;
    let installed_now = downloads.store.installed().iter().any(|g| g.app_name == game.app_name);
    if !still_queueable(installed_now, downloads.has_job(&game.app_name), install.checking.is_busy(&game.app_name), downloads.is_wiping()) {
        return Err(epic_import::being_checked(&game.title));
    }
    downloads
        .store
        .save_job(&job, &p.bytes)
        .map_err(|e| InstallError::Io { folder: path.display().to_string(), detail: e.to_string() }.user_message())?;
    install.prepared.lock().unwrap_or_else(|e| e.into_inner()).remove(&game.app_name);
    info!("install queued: {} ({} bytes to download) into {}", job.app_name, job.download_size, path.display());
    downloads.enqueue(&app, job, p.manifest, None);
    Ok(downloads.view())
}

/// The Install box closed without installing: drop the file list it fetched (it can be large).
#[tauri::command]
fn install_forget(app_name: String, install: tauri::State<'_, AppInstall>) {
    install.prepared.lock().unwrap_or_else(|e| e.into_inner()).remove(&app_name);
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

/// The installed game's folder, only from its record, and only if it's still there (spec §6p).
fn game_folder(installed: &[InstalledGame], app_name: &str) -> Result<std::path::PathBuf, String> {
    let game = installed.iter().find(|g| g.app_name == app_name).ok_or_else(|| "Refresh your Library, then try again.".to_string())?;
    if game.install_path.is_dir() {
        Ok(game.install_path.clone())
    } else {
        Err(format!("Couldn't find {}'s folder. Uninstall it, then install it again.", game.title))
    }
}

/// Explorer's argument: the folder in quotes. Explorer splits an unquoted argument at commas, and
/// Rust only adds quotes for spaces. A Windows path can't contain a quote.
fn explorer_arg(folder: &std::path::Path) -> String {
    format!("\"{}\"", folder.display())
}

/// "Open game folder" in the ⋯ menu: Windows Explorer in the game's folder (spec §6p).
#[tauri::command]
fn open_game_folder(app_name: String, install: tauri::State<'_, AppInstall>) -> Result<(), String> {
    let folder = game_folder(&install.downloads.store.installed(), &app_name)?;
    let mut explorer = std::process::Command::new("explorer.exe");
    #[cfg(windows)]
    std::os::windows::process::CommandExt::raw_arg(&mut explorer, explorer_arg(&folder));
    #[cfg(not(windows))]
    explorer.arg(&folder);
    explorer.spawn().map_err(|_| "Couldn't open the folder.".to_string())?;
    info!("opened a game folder");
    Ok(())
}

#[tauri::command]
async fn uninstall_game(
    app: tauri::AppHandle,
    app_name: String,
    install: tauri::State<'_, AppInstall>,
    running: tauri::State<'_, running::AppRunning>,
) -> Result<Vec<InstalledGame>, String> {
    if running.contains(&app_name) {
        let title = install.downloads.store.installed().into_iter().find(|g| g.app_name == app_name).map(|g| g.title).unwrap_or_else(|| app_name.clone());
        return Err(fishy_core::running::close_first(&title));
    }
    // The game's add-on pieces go with it, first (spec §6n).
    let mut names: Vec<String> = install.downloads.store.installed().into_iter().filter(|g| g.parent.as_deref() == Some(app_name.as_str())).map(|g| g.app_name).collect();
    names.push(app_name.clone());
    // A paused update goes with the game (spec §6e); a running one must be paused first.
    if !names.iter().all(|n| install.downloads.take_stopped_update(n)) {
        return Err("Pause the update first, then uninstall.".to_string());
    }
    let _ = app.emit("downloads-changed", install.downloads.view());
    let store = install.downloads.store.clone();
    tauri::async_runtime::spawn_blocking(move || names.iter().try_for_each(|n| uninstall(&store, n)))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| {
            let m = e.user_message();
            warn!("uninstall of {app_name} failed: {m}");
            m
        })?;
    info!("uninstalled {app_name}");
    let _ = app.emit("installed-changed", ());
    Ok(install.downloads.store.installed())
}

#[tauri::command]
fn open_login_in_browser() -> Result<(), String> {
    login_window::open_in_browser().map_err(|_| BROWSER_ERROR.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        // Fishy Client's own signed updates (spec §6j); used from Rust only, so the page gets no permission.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .setup(|app| {
            // Logs first, so everything after is recorded. Without a logs folder the app still runs.
            let guard = fishy_core::logs::init(&fishy_core::logs::default_dir()).ok();
            app.manage(data::AppLogs(guard));
            info!("Fishy Client {} started", app.package_info().version);
            app.manage(AppAuth {
                manager: AuthManager::new(EpicClient::new(), KeyringStore::windows(), chrono_now),
                current: Mutex::new(None),
            });
            app.manage(AppLibrary { client: LibraryClient::new(), cache: LibraryCache::default_location(), achievements: AchievementsClient::new() });
            app.manage(AppInstall {
                downloads: Downloads::new(InstallStore::default_location(), InstallClient::new()),
                prepared: std::sync::Mutex::new(HashMap::new()),
                checking: epic_import::Checking::default(),
            });
            app.manage(play::AppLaunch::new());
            app.manage(running::AppRunning::default());
            app.manage(store_window::AppStore::new());
            let settings = updates::AppSettings { store: fishy_core::settings::SettingsStore::default_location() };
            // The saved speed limit applies from the first download, not only after a change.
            app.state::<AppInstall>().downloads.limiter.set(fishy_core::install::limit::limit_bytes(&settings.store.load()));
            app.manage(settings);
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
            open_game_folder,
            game_achievements,
            store_window::store_free_games,
            store_window::store_open,
            open_login_in_browser,
            library_cached,
            library_refresh,
            install_options,
            install_start,
            install_forget,
            pick_folder,
            downloads_state,
            download_pause,
            download_resume,
            download_cancel,
            installed_list,
            uninstall_game,
            play::play_game,
            running::running_list,
            updates::update_start,
            updates::settings_get,
            updates::settings_set,
            data::open_logs_folder,
            data::delete_all_data,
            data::autostart_get,
            data::autostart_set,
            data::default_install_folder,
            data::log_ui_error,
            app_update::app_update_check,
            app_update::app_update_install,
            epic_import::epic_import_scan,
            epic_import::epic_import_add,
            epic_import::epic_import_repair,
            epic_folder
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn chrono_now() -> fishy_core::chrono::DateTime<fishy_core::chrono::Utc> {
    fishy_core::chrono::Utc::now()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explorer_gets_the_folder_in_quotes() {
        // Explorer splits an unquoted argument at commas, and Rust only quotes paths with spaces.
        assert_eq!(explorer_arg(std::path::Path::new(r"C:\Games\Kelp,Runner")), r#""C:\Games\Kelp,Runner""#);
    }

    #[test]
    fn namespace_comes_from_the_cached_library() {
        let g = Game {
            app_name: "Tide".into(),
            namespace: "ns-tide".into(),
            catalog_item_id: "c".into(),
            title: "Tide".into(),
            cover_url: None,
            installable: true,
            launch: Default::default(),
        };
        assert_eq!(namespace_of(std::slice::from_ref(&g), "Tide"), Some("ns-tide".to_string()));
        assert_eq!(namespace_of(&[g], "Other"), None);
    }

    #[test]
    fn missing_folder_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let mut g = updates_test_record("Tide", dir.path());
        assert_eq!(game_folder(std::slice::from_ref(&g), "Tide"), Ok(dir.path().to_path_buf()));
        g.install_path = dir.path().join("gone");
        assert_eq!(game_folder(&[g], "Tide"), Err("Couldn't find Tide's folder. Uninstall it, then install it again.".to_string()));
        assert!(game_folder(&[], "Tide").is_err());
    }

    fn updates_test_record(app: &str, path: &std::path::Path) -> InstalledGame {
        InstalledGame {
            app_name: app.into(),
            title: app.into(),
            version: "1".into(),
            install_path: path.to_path_buf(),
            executable: String::new(),
            launch_parameters: String::new(),
            install_size: 1,
            installed_at: fishy_core::chrono::Utc::now(),
            last_played: None,
            playtime_secs: 0,
            parts: None,
            parent: None,
            namespace: String::new(),
            catalog_item_id: String::new(),
        }
    }

    #[test]
    fn a_refresh_is_saved_only_for_the_account_still_signed_in() {
        let fan = AuthState::SignedIn { display_name: "Fan".into(), account_id: "a1".into() };
        let other = AuthState::SignedIn { display_name: "Other".into(), account_id: "b2".into() };
        let offline = AuthState::Offline { display_name: "Fan".into() };
        assert!(may_save_refresh(Some(&fan), Some(&fan)));
        assert!(may_save_refresh(Some(&offline), Some(&fan)), "back online while it ran");
        assert!(!may_save_refresh(Some(&fan), Some(&AuthState::SignedOut)), "expired or logged out");
        assert!(!may_save_refresh(Some(&fan), Some(&other)), "someone else signed in");
        assert!(!may_save_refresh(Some(&fan), None));
    }

    #[test]
    fn the_installer_is_one_msi_with_a_fixed_upgrade_code() {
        // Spec §6f.6: a newer .msi must replace the old version, never install a second copy.
        let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let bundle = &conf["bundle"];
        assert_eq!(conf["productName"], "Fishy Client");
        // Version 1 (spec §1, decided 2026-11-22); the program and the installer must agree.
        assert_eq!(conf["version"], "1.7.0");
        assert_eq!(conf["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(bundle["targets"], serde_json::json!(["msi"]));
        assert_eq!(bundle["publisher"], "Fishy Client");
        assert_eq!(bundle["license"], "GPL-3.0-or-later");
        assert_eq!(bundle["windows"]["wix"]["upgradeCode"], "d895a25e-2ca5-44d4-b204-1ee4cb3b89f9");
        assert_eq!(bundle["windows"]["wix"]["language"], "en-US");
        // Spec §6j: signed updates from the public repository's newest Release.
        // Signing is on only for release builds (tauri.release.conf.json), so a plain build works
        // without the maintainer's key.
        assert_ne!(bundle["createUpdaterArtifacts"], true);
        let release: serde_json::Value = serde_json::from_str(include_str!("../tauri.release.conf.json")).unwrap();
        assert_eq!(release, serde_json::json!({ "bundle": { "createUpdaterArtifacts": true } }));
        let updater = &conf["plugins"]["updater"];
        assert_eq!(updater["endpoints"], serde_json::json!(["https://github.com/Gnammahh/fishy-client/releases/latest/download/latest.json"]));
        assert_eq!(updater["windows"]["installMode"], "passive");
        assert!(updater["pubkey"].as_str().is_some_and(|k| !k.is_empty() && k != "REPLACE_WITH_PUBLIC_KEY"));
    }

    #[test]
    fn folder_fields_ask_for_permission_when_it_is_safe() {
        let no = || Err(std::path::PathBuf::from(r"C:\Program Files\Epic Games"));
        assert_eq!(folder_fields(Ok(()), true), (true, false, None));
        assert_eq!(folder_fields(no(), true), (false, true, None));
        let (ok, ask, msg) = folder_fields(no(), false);
        assert_eq!((ok, ask), (false, false));
        assert!(msg.unwrap().contains("without administrator rights"));
    }

    #[test]
    fn nothing_else_may_have_queued_the_game_during_the_prompt() {
        assert!(still_queueable(false, false, false, false));
        assert!(!still_queueable(true, false, false, false), "installed meanwhile");
        assert!(!still_queueable(false, true, false, false), "queued meanwhile");
        assert!(!still_queueable(false, false, true, false), "being imported");
        assert!(!still_queueable(false, false, false, true), "Delete all my data running");
    }

    #[test]
    fn after_permission_refuses_when_still_not_writable() {
        assert!(after_permission(true, r"D:\Games\Tide").is_ok());
        assert_eq!(
            after_permission(false, r"D:\Games\Tide").unwrap_err().user_message(),
            r"Couldn't create D:\Games\Tide. Pick a different folder."
        );
    }

    #[test]
    fn library_and_browser_messages_say_what_to_do() {
        assert_eq!(LIBRARY_ERROR, "Couldn't load your library from Epic. Check your internet, then press Try again.");
        assert_eq!(BROWSER_ERROR, "Couldn't open your browser. Use Sign in with Epic instead.");
    }
}
