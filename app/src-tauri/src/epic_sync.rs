//! Keeping Epic's launcher's list in step with installed games (spec §6h). Nothing here ever
//! fails an install, update, import or uninstall: problems only go to the log.
use crate::{AppInstall, AppLibrary};
use fishy_core::epic_import::epic_manifests_dir;
use fishy_core::epic_sync::{new_guid, remove_entry, write_entry, Mode, SyncInfo, Synced};
use fishy_core::install::store::{InstallStore, InstalledGame};
use fishy_core::library::Game;
use fishy_core::settings::Settings;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::Manager;
use tracing::{info, warn};

/// One writer at a time, so a start-up sync and an install finishing can't write the same entry.
static WRITING: Mutex<()> = Mutex::new(());

/// The switch went from off to on (spec §6h: then every installed game is added).
pub(crate) fn turned_on(old: &Settings, new: &Settings) -> bool {
    !old.show_in_epic && new.show_in_epic
}

pub(crate) fn sync_info(game: &Game) -> SyncInfo {
    SyncInfo {
        namespace: game.namespace.clone(),
        catalog_item_id: game.catalog_item_id.clone(),
        can_run_offline: game.launch.can_run_offline,
        requires_ot: game.launch.requires_ot,
    }
}

/// App names only, never paths (spec §12).
pub(crate) fn failed_line(apps: &[String]) -> String {
    let n = apps.len();
    let games = if n == 1 { "game" } else { "games" };
    format!("epic sync: couldn't update Epic's list for {n} {games} ({})", apps.join(", "))
}

/// Epic's folder, when the switch is on and Epic's launcher is installed.
fn target(app: &tauri::AppHandle) -> Option<PathBuf> {
    let on = app.state::<crate::updates::AppSettings>().store.load().show_in_epic;
    epic_manifests_dir().filter(|d| on && d.is_dir())
}

/// Writes the entries for these installed games. Returns how many Epic now lists, and the
/// app names that couldn't be written. A game missing from the saved Library is skipped (the
/// Library isn't saved yet, or it's signed out); a refresh syncs again.
fn write_all(epic: &std::path::Path, store: &InstallStore, games: &[InstalledGame], library: &[Game], mode: Mode) -> (usize, Vec<String>) {
    let _one = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    let (mut listed, mut failed) = (0, Vec::new());
    for g in games {
        let Some(l) = library.iter().find(|l| l.app_name == g.app_name) else { continue };
        let written = store.installed_manifest(&g.app_name).and_then(|bytes| write_entry(epic, g, &sync_info(l), &bytes, mode, new_guid));
        match written {
            Ok(Synced::Written { .. } | Synced::Kept) => listed += 1,
            Ok(Synced::OtherCopy | Synced::NoEpic | Synced::NoGame) => {}
            Err(_) => failed.push(g.app_name.clone()),
        }
    }
    (listed, failed)
}

async fn sync(app: &tauri::AppHandle, only: Option<&str>) {
    let Some(epic) = target(app) else { return };
    let store = app.state::<AppInstall>().downloads.store.clone();
    let library = app.state::<AppLibrary>().cache.load().map(|c| c.games).unwrap_or_default();
    let games: Vec<InstalledGame> = store.installed().into_iter().filter(|g| only.is_none_or(|a| a == g.app_name)).collect();
    if games.is_empty() {
        return;
    }
    let all = only.is_none();
    // A whole sync never rolls back an entry Epic updated itself (spec §6h).
    let mode = if all { Mode::AddOnly } else { Mode::Update };
    let result = tauri::async_runtime::spawn_blocking(move || write_all(&epic, &store, &games, &library, mode)).await;
    match result {
        Ok((listed, failed)) => {
            if !failed.is_empty() {
                warn!("{}", failed_line(&failed));
            }
            if all {
                info!("epic sync: {listed} games in Epic's list");
            }
        }
        Err(_) => warn!("epic sync: stopped unexpectedly"),
    }
}

/// After an install, update or import finished.
pub(crate) async fn sync_one(app: &tauri::AppHandle, app_name: &str) {
    sync(app, Some(app_name)).await;
}

/// At start-up, when the switch is turned on and after a Library refresh: adds missing entries.
pub(crate) async fn sync_all(app: &tauri::AppHandle) {
    sync(app, None).await;
}

/// After an uninstall. `game` is the record as it was before.
pub(crate) async fn remove_one(app: &tauri::AppHandle, game: InstalledGame) {
    let Some(epic) = target(app) else { return };
    let name = game.app_name.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let _one = WRITING.lock().unwrap_or_else(|e| e.into_inner());
        remove_entry(&epic, &game)
    })
    .await;
    if !matches!(result, Ok(Ok(_))) {
        warn!("{}", failed_line(&[name]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn turned_on_only_from_off_to_on() {
        let on = Settings::default();
        let off = Settings { show_in_epic: false, ..Settings::default() };
        assert!(turned_on(&off, &on));
        assert!(!turned_on(&on, &on));
        assert!(!turned_on(&on, &off));
        assert!(!turned_on(&off, &off));
    }

    #[test]
    fn sync_info_from_library_game() {
        let mut g = Game {
            app_name: "A".into(),
            namespace: "ns".into(),
            catalog_item_id: "cat".into(),
            title: "T".into(),
            cover_url: None,
            installable: true,
            launch: Default::default(),
        };
        g.launch.requires_ot = true;
        g.launch.can_run_offline = false;
        assert_eq!(sync_info(&g), SyncInfo { namespace: "ns".into(), catalog_item_id: "cat".into(), can_run_offline: false, requires_ot: true });
    }

    fn library_game(app: &str) -> Game {
        Game {
            app_name: app.into(),
            namespace: "ns".into(),
            catalog_item_id: "cat".into(),
            title: app.into(),
            cover_url: None,
            installable: true,
            launch: Default::default(),
        }
    }

    /// Games A and B installed in a temp folder, with Epic's list folder next to them.
    fn installed_two() -> (tempfile::TempDir, InstallStore, PathBuf) {
        use fishy_core::install::manifest::testbuild::{build_binary, sample};
        use fishy_core::install::store::JobInfo;
        let dir = tempfile::tempdir().unwrap();
        let store = InstallStore::at(dir.path().join("data"));
        for app in ["A", "B"] {
            let job = JobInfo {
                app_name: app.into(),
                title: app.into(),
                namespace: "ns".into(),
                catalog_item_id: "cat".into(),
                install_path: dir.path().join("Games").join(app),
                download_size: 1,
                install_size: 1,
                base_urls: vec![],
                update_from: None,
                imported: false,
            };
            std::fs::create_dir_all(&job.install_path).unwrap();
            store.save_job(&job, &build_binary(&sample(), true)).unwrap();
            store.finish_install(&job, &sample()).unwrap();
        }
        let epic = dir.path().join("Manifests");
        std::fs::create_dir_all(&epic).unwrap();
        (dir, store, epic)
    }

    #[test]
    fn write_all_skips_games_missing_from_the_library() {
        let (_dir, store, epic) = installed_two();
        let (listed, failed) = write_all(&epic, &store, &store.installed(), &[library_game("A")], Mode::AddOnly);
        assert_eq!(listed, 1);
        assert!(failed.is_empty(), "B isn't in the saved Library: skipped, not a failure");
        assert_eq!(std::fs::read_dir(&epic).unwrap().count(), 1);
    }

    #[test]
    fn write_all_counts_a_game_epic_already_lists() {
        let (_dir, store, epic) = installed_two();
        let lib = [library_game("A"), library_game("B")];
        write_all(&epic, &store, &store.installed(), &lib, Mode::AddOnly);
        let (listed, failed) = write_all(&epic, &store, &store.installed(), &lib, Mode::AddOnly);
        assert_eq!((listed, failed.len()), (2, 0));
        assert_eq!(std::fs::read_dir(&epic).unwrap().count(), 2, "no duplicates");
    }

    #[test]
    fn failed_line_counts_and_names() {
        assert_eq!(failed_line(&["A".into()]), "epic sync: couldn't update Epic's list for 1 game (A)");
        assert_eq!(failed_line(&["A".into(), "B".into()]), "epic sync: couldn't update Epic's list for 2 games (A, B)");
    }
}
