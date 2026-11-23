//! Adding games already installed by Epic's own launcher (spec §6g).
use crate::downloads::DownloadsView;
use crate::{AppAuth, AppInstall, AppLibrary};
use fishy_core::epic_import::{self as epic, EpicItem};
use fishy_core::install::client::ManifestInfo;
use fishy_core::install::manifest::Manifest;
use fishy_core::install::parts::{chosen_on_disk, only_parts};
use fishy_core::install::store::InstalledGame;
use fishy_core::install::InstallError;
use fishy_core::library::Game;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use tracing::{info, warn};

const REFRESH: &str = "Refresh your Library, then try again.";

fn unreadable(title: &str) -> String {
    format!("Couldn't read {title}'s files from Epic's launcher. Press Install to download it.")
}

fn offline(title: &str) -> String {
    format!("Couldn't reach Epic to check {title}. Check your internet.")
}

pub(crate) fn being_checked(title: &str) -> String {
    format!("Fishy Client is checking {title}'s files from Epic's launcher. Try again in a moment.")
}

/// Games being checked or queued for an import right now. Taking one reserves it until the
/// returned guard is dropped, so Add, Download them and Install never write the same game at once.
#[derive(Default, Clone)]
pub struct Checking(Arc<Mutex<HashSet<String>>>);

pub struct CheckingGuard {
    set: Arc<Mutex<HashSet<String>>>,
    app: String,
}

impl Drop for CheckingGuard {
    fn drop(&mut self) {
        self.set.lock().unwrap_or_else(|e| e.into_inner()).remove(&self.app);
    }
}

impl Checking {
    pub fn take(&self, app: &str) -> Option<CheckingGuard> {
        let fresh = self.0.lock().unwrap_or_else(|e| e.into_inner()).insert(app.to_string());
        fresh.then(|| CheckingGuard { set: self.0.clone(), app: app.to_string() })
    }

    pub fn is_busy(&self, app: &str) -> bool {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).contains(app)
    }
}

/// Checked again right before anything is written: nothing else installed or queued it meanwhile.
fn still_free(app: &str, installed: &[InstalledGame], has_job: impl Fn(&str) -> bool) -> bool {
    !installed.iter().any(|g| g.app_name == app) && !has_job(app)
}

/// An add-on piece already recorded by an earlier Add or Download them: left as it is, so it
/// never blocks its game from being added (spec §6n).
fn recorded_piece(app: &str, parent: Option<&str>, installed: &[InstalledGame]) -> bool {
    parent.is_some() && installed.iter().any(|g| g.app_name == app)
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

fn piece_missing(title: &str) -> String {
    format!("Couldn't import {title}: a part of it is missing. Open it once in Epic's launcher, then try again.")
}

/// Epic's games that are in the Library, not installed here and not in the Downloads list.
/// Add-on pieces (Fortnite's Save the World Content) come with their game, never on their own.
fn candidates(items: Vec<EpicItem>, games: &[Game], installed: &[InstalledGame], has_job: impl Fn(&str) -> bool) -> Vec<(EpicItem, Game)> {
    items
        .into_iter()
        .filter(|i| i.parent.is_none() && !installed.iter().any(|g| g.app_name == i.app_name) && !has_job(&i.app_name))
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
async fn manifest_for(item: &EpicItem, auth: &AppAuth, install: &AppInstall, need_info: bool) -> Result<(Vec<u8>, Manifest, Option<ManifestInfo>), InstallError> {
    let local = epic::local_manifest(item).and_then(|b| epic::usable_manifest(item, &b).map(|m| (b, m)));
    if let (Some((bytes, m)), false) = (&local, need_info) {
        return Ok((bytes.clone(), m.clone(), None));
    }
    let client = &install.downloads.client;
    let token = auth.manager.access_token().await?;
    let info = client.manifest_info(&token, &item.namespace, &item.catalog_item_id, &item.app_name).await?;
    if let Some((bytes, m)) = local {
        return Ok((bytes, m, Some(info)));
    }
    info!("epic import: {} has no usable file list, downloading Epic's", item.app_name);
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

/// The game or one of its add-on pieces, ready to check: its manifest filtered to the parts on
/// disk (spec §6n).
struct Piece {
    item: EpicItem,
    title: String,
    bytes: Vec<u8>,
    m: Manifest,
    parts: Option<Vec<String>>,
    base_urls: Vec<String>,
}

/// The game first, then its pieces. A piece whose file list can't be had refuses the whole game.
async fn pieces(app: &tauri::AppHandle, item: &EpicItem, game: &Game, auth: &AppAuth, install: &AppInstall, need_info: bool) -> Result<Vec<Piece>, (String, bool)> {
    let all = epic::epic_manifests_dir().map(|d| epic::scan(&d)).unwrap_or_default();
    let list: Vec<(EpicItem, String)> =
        std::iter::once((item.clone(), game.title.clone())).chain(epic::pieces_of(&all, item).into_iter().map(|p| (p.clone(), p.title.clone()))).collect();
    let mut out = Vec::new();
    for (n, (it, title)) in list.into_iter().enumerate() {
        let (bytes, m, info) = match manifest_for(&it, auth, install, need_info).await {
            Ok(x) => x,
            Err(e) if n == 0 || matches!(e, InstallError::Expired) => return Err(failure(app, &game.title, e).await),
            Err(_) => return Err((piece_missing(&game.title), false)),
        };
        let (root, m2) = (it.install_path.clone(), m.clone());
        let parts = tokio::task::spawn_blocking(move || chosen_on_disk(&root, &m2)).await.unwrap_or(None);
        if let Some(p) = &parts {
            let names: HashSet<&String> = m.files.iter().flat_map(|f| &f.install_tags).collect();
            info!("epic import: {} with optional parts ({} of {} part names)", it.app_name, p.len(), names.len());
        }
        let m = only_parts(m, parts.as_deref());
        out.push(Piece { item: it, title, bytes, m, parts, base_urls: info.map(|i| i.base_urls).unwrap_or_default() });
    }
    Ok(out)
}

/// Checks one game (with its pieces) and adds it if every file is there.
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
    let Some(_checking) = install.checking.take(&app_name) else { return error(being_checked(&game.title), false) };
    let mut list = match pieces(&app, &item, &game, &auth, &install, false).await {
        Ok(l) => l,
        Err((message, signed_out)) => return error(message, signed_out),
    };
    let before = install.downloads.store.installed();
    list.retain(|p| !recorded_piece(&p.item.app_name, p.item.parent.as_deref(), &before));
    let (mut files, mut bytes) = (0, 0);
    for p in &list {
        let result = check(&p.item, &p.m).await;
        files += result.bad_files;
        bytes += result.bad_bytes;
    }
    if files > 0 {
        info!("epic import: {app_name} has {files} files to download");
        return Ok(AddOutcome::Missing { files, bytes });
    }
    let store = &install.downloads.store;
    let installed = store.installed();
    if list.iter().any(|p| !still_free(&p.item.app_name, &installed, |a| install.downloads.has_job(a))) {
        return error(REFRESH.into(), false);
    }
    // Pieces first: the game only shows as installed once everything it needs is recorded.
    for p in list.iter().rev() {
        let job = epic::import_job(&p.item, &p.title, &p.m, Vec::new(), p.parts.clone());
        if let Err(e) = epic::add_installed(store, &job, &p.bytes, &p.m) {
            let _ = store.remove_job(&p.item.app_name);
            warn!("epic import: couldn't save {}: {}", p.item.app_name, e.kind());
            return error(unreadable(&game.title), false);
        }
    }
    info!("epic import: added {app_name}");
    Ok(AddOutcome::Added)
}

/// "Download them": queues a download of only the missing or damaged files, one per piece.
/// Pieces with nothing missing are recorded as installed right away.
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
    let Some(_checking) = install.checking.take(&app_name) else { return Err(being_checked(&game.title)) };
    let mut list = pieces(&app, &item, &game, &auth, &install, true).await.map_err(|(m, _)| m)?;
    let before = downloads.store.installed();
    list.retain(|p| !recorded_piece(&p.item.app_name, p.item.parent.as_deref(), &before));
    let mut checked = Vec::new();
    for p in list {
        let result = check(&p.item, &p.m).await;
        checked.push((p, result));
    }
    let installed = downloads.store.installed();
    if checked.iter().any(|(p, _)| !still_free(&p.item.app_name, &installed, |a| downloads.has_job(a))) {
        return Err(REFRESH.into());
    }
    for (p, result) in checked.into_iter().rev() {
        let job = epic::import_job(&p.item, &p.title, &p.m, p.base_urls.clone(), p.parts.clone());
        if result.all_good() {
            if let Err(e) = epic::add_installed(&downloads.store, &job, &p.bytes, &p.m) {
                let _ = downloads.store.remove_job(&p.item.app_name);
                warn!("epic import: couldn't save {}: {}", p.item.app_name, e.kind());
                return Err(unreadable(&game.title));
            }
            continue;
        }
        if let Err(e) = epic::save_repair(&downloads.store, &job, &p.bytes, &result) {
            let _ = downloads.store.remove_job(&p.item.app_name);
            warn!("epic import: couldn't save the repair of {}: {}", p.item.app_name, e.kind());
            return Err(unreadable(&game.title));
        }
        info!("epic import: repair of {} queued ({} files)", p.item.app_name, result.bad_files);
        downloads.enqueue(&app, job, Arc::new(p.m), None);
    }
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
            parent: None,
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
            playtime_secs: 0,
            options: Default::default(),
            parts: None,
            parent: None,
            namespace: String::new(),
            catalog_item_id: String::new(),
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
    fn a_game_being_checked_is_reserved_until_done() {
        let r = Checking::default();
        let first = r.take("A").expect("free");
        assert!(r.is_busy("A"));
        assert!(r.take("A").is_none(), "a second Add or Download them waits");
        assert!(r.take("B").is_some(), "other games aren't blocked");
        drop(first);
        assert!(!r.is_busy("A"));
        assert!(r.take("A").is_some());
    }

    #[test]
    fn still_free_rechecks_installed_and_queued() {
        assert!(still_free("A", &[], |_| false));
        assert!(!still_free("A", &[record("A")], |_| false));
        assert!(!still_free("A", &[], |app| app == "A"));
    }

    #[test]
    fn candidates_skip_add_on_pieces() {
        let mut piece = item("aa31");
        piece.parent = Some("Fortnite".into());
        let got = candidates(vec![item("Fortnite"), piece], &[game("Fortnite"), game("aa31")], &[], |_| false);
        let names: Vec<&str> = got.iter().map(|(i, _)| i.app_name.as_str()).collect();
        assert_eq!(names, vec!["Fortnite"], "a piece is imported with its game, never on its own");
    }

    #[test]
    fn a_recorded_piece_is_left_alone_on_the_next_add() {
        let mut piece = record("aa31");
        piece.parent = Some("Fortnite".into());
        assert!(recorded_piece("aa31", Some("Fortnite"), &[piece.clone()]), "kept, so it doesn't block the game");
        assert!(!recorded_piece("Fortnite", None, &[record("Fortnite")]), "the game itself is never skipped");
        assert!(!recorded_piece("bb", Some("Fortnite"), &[piece]));
    }

    #[test]
    fn candidates_skip_installed_and_queued() {
        let got = candidates(vec![item("A"), item("B"), item("C")], &[game("A"), game("B"), game("C")], &[record("A")], |app| app == "B");
        let names: Vec<&str> = got.iter().map(|(i, _)| i.app_name.as_str()).collect();
        assert_eq!(names, vec!["C"]);
    }
}
