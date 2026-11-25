//! Removing games and cancelled downloads. Only files the manifest lists are deleted, then folders
//! left empty (deepest first), so anything the user added to the game folder stays.
use super::manifest::{parse_manifest, Manifest};
use super::store::{file_path, InstallStore, JobInfo};
use super::InstallError;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The folder in the game's folder where updates are built (spec §6u).
pub const ASIDE: &str = ".fishy-update";

/// Where `app`'s update builds its files: `<game folder>\.fishy-update\<app>`.
pub fn aside_dir(root: &Path, app: &str) -> PathBuf {
    root.join(ASIDE).join(app)
}

/// Deletes `app`'s side folder, and `.fishy-update` if nothing else is in it. False if it couldn't.
pub fn delete_aside(root: &Path, app: &str) -> bool {
    let ok = match std::fs::remove_dir_all(aside_dir(root, app)) {
        Err(e) => e.kind() == std::io::ErrorKind::NotFound,
        Ok(()) => true,
    };
    let _ = std::fs::remove_dir(root.join(ASIDE));
    ok
}

/// Whether Cancel may stop this download (spec §6u): always for installs; for an update, only
/// while every file it finished is still aside (none is in the game yet).
pub fn update_can_be_cancelled(store: &InstallStore, job: &JobInfo) -> bool {
    job.update_from.is_none()
        || store.finished_names(job).iter().all(|n| file_path(&aside_dir(&job.install_path, &job.app_name), n).is_ok_and(|p| p.is_file()))
}

/// Cancel of an update: what it built aside and its saved progress go; the game keeps its version.
pub fn discard_update(store: &InstallStore, job: &JobInfo) -> Result<(), InstallError> {
    if !delete_aside(&job.install_path, &job.app_name) {
        return Err(InstallError::InUse);
    }
    store.remove_job(&job.app_name).map_err(|e| InstallError::Io { folder: job.install_path.display().to_string(), detail: e.to_string() })
}

/// Deletes the manifest's files under `root` and the folders left empty. False if a file that
/// exists couldn't be deleted (for example, the game is running).
pub fn delete_manifest_files(root: &Path, m: &Manifest) -> bool {
    delete_named_files(root, m.files.iter().map(|f| f.filename.as_str()), true)
}

/// Deletes the named files under `root`, then the folders left empty (and `root` itself if
/// `remove_root` and it's empty). False if a file that exists couldn't be deleted.
pub fn delete_named_files<'a>(root: &Path, names: impl IntoIterator<Item = &'a str>, remove_root: bool) -> bool {
    let mut ok = true;
    let mut dirs: BTreeSet<PathBuf> = BTreeSet::new();
    for name in names {
        // Names that would escape the folder were never written, so there's nothing to delete.
        let Ok(path) = file_path(root, name) else { continue };
        let mut dir = path.parent();
        while let Some(d) = dir.filter(|d| d.starts_with(root) && *d != root) {
            dirs.insert(d.to_path_buf());
            dir = d.parent();
        }
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => ok = false,
            _ => {}
        }
    }
    // Deepest first; remove_dir only removes empty folders, so the user's own files keep theirs.
    let mut dirs: Vec<PathBuf> = dirs.into_iter().collect();
    dirs.sort_by_key(|d| std::cmp::Reverse(d.components().count()));
    for d in dirs.iter().map(PathBuf::as_path).chain(remove_root.then_some(root)) {
        let _ = std::fs::remove_dir(d);
    }
    ok
}

pub fn uninstall(store: &InstallStore, app: &str) -> Result<(), InstallError> {
    let game = store.installed().into_iter().find(|g| g.app_name == app).ok_or(InstallError::NotInstalled)?;
    let io_err = |e: std::io::Error| InstallError::Io { folder: game.install_path.display().to_string(), detail: e.to_string() };
    // No saved manifest (damaged data folder): forget the record; the files stay for the user to delete.
    // A paused update may already have written files only the new version lists (spec §6e).
    let manifests = [store.installed_manifest(app), store.job_manifest(app)];
    let names: Vec<String> = manifests
        .iter()
        .filter_map(|b| b.as_ref().ok().and_then(|b| parse_manifest(b).ok()))
        .flat_map(|m| m.files.into_iter().map(|f| f.filename))
        .collect();
    // A paused update's side folder goes first (spec §6u); the game folder may then be empty.
    if !delete_aside(&game.install_path, app) || !delete_named_files(&game.install_path, names.iter().map(String::as_str), true) {
        return Err(InstallError::InUse);
    }
    store.remove_job(app).map_err(io_err)?;
    store.forget_installed(app).map_err(io_err)
}

/// Cancel: deletes what a download wrote so far and its saved progress.
/// An imported game's files were there before (spec §6g), so only the saved progress goes.
pub fn discard_download(store: &InstallStore, job: &JobInfo) -> Result<(), InstallError> {
    let manifest = if job.imported { None } else { store.job_manifest(&job.app_name).ok().and_then(|b| parse_manifest(&b).ok()) };
    if let Some(manifest) = manifest {
        if !delete_manifest_files(&job.install_path, &manifest) {
            return Err(InstallError::InUse);
        }
    }
    store
        .remove_job(&job.app_name)
        .map_err(|e| InstallError::Io { folder: job.install_path.display().to_string(), detail: e.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::manifest::testbuild::{build_binary, sample};
    use std::path::PathBuf;

    struct Env {
        _dir: tempfile::TempDir,
        store: InstallStore,
        job: JobInfo,
    }

    /// The sample game, "installed" by writing its files by hand.
    fn installed() -> Env {
        let dir = tempfile::tempdir().unwrap();
        let store = InstallStore::at(dir.path().join("data"));
        let root = dir.path().join("Games").join("Tidepool");
        let job = JobInfo {
            app_name: "Tide".into(),
            title: "Tidepool".into(),
            namespace: "ns".into(),
            catalog_item_id: "cat".into(),
            install_path: root.clone(),
            download_size: 1,
            install_size: 1,
            base_urls: vec![],
            update_from: None,
            imported: false,
            parts: None,
            parent: None,
        };
        write_files(&root);
        store.save_job(&job, &build_binary(&sample(), true)).unwrap();
        Env { _dir: dir, store, job }
    }

    fn write_files(root: &Path) {
        for f in &sample().files {
            let p = crate::install::store::file_path(root, &f.filename).unwrap();
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b"x").unwrap();
        }
    }

    fn root(e: &Env) -> PathBuf {
        e.job.install_path.clone()
    }

    #[test]
    fn cancel_of_imported_job_keeps_game_files() {
        let mut e = installed();
        e.job.imported = true;
        e.store.save_job(&e.job, &build_binary(&sample(), true)).unwrap();
        discard_download(&e.store, &e.job).unwrap();
        for f in &sample().files {
            assert!(crate::install::store::file_path(&root(&e), &f.filename).unwrap().is_file(), "{} kept", f.filename);
        }
        assert!(e.store.pending_jobs().is_empty(), "the download folder is gone");
    }

    #[test]
    fn uninstall_removes_files_and_empty_dirs() {
        let e = installed();
        e.store.finish_install(&e.job, &sample()).unwrap();
        uninstall(&e.store, "Tide").unwrap();
        assert!(!root(&e).exists(), "empty game folder is removed too");
        assert!(root(&e).parent().unwrap().exists(), "the Games folder stays");
        assert!(e.store.installed().is_empty());
        assert!(e.store.installed_manifest("Tide").is_err());
    }

    #[test]
    fn uninstall_deletes_a_paused_updates_side_folder() {
        let e = installed();
        e.store.finish_install(&e.job, &sample()).unwrap();
        let aside = aside_dir(&root(&e), "Tide").join("Content");
        std::fs::create_dir_all(&aside).unwrap();
        std::fs::write(aside.join("new.pak"), b"half").unwrap();
        uninstall(&e.store, "Tide").unwrap();
        assert!(!root(&e).exists(), "nothing left, the side folder included");
    }

    #[test]
    fn uninstall_keeps_user_files() {
        let e = installed();
        e.store.finish_install(&e.job, &sample()).unwrap();
        std::fs::create_dir_all(root(&e).join("saves")).unwrap();
        std::fs::write(root(&e).join("saves").join("my.sav"), b"mine").unwrap();
        std::fs::write(root(&e).join("Binaries").join("mod.dll"), b"mine").unwrap();
        uninstall(&e.store, "Tide").unwrap();
        assert_eq!(std::fs::read(root(&e).join("saves").join("my.sav")).unwrap(), b"mine");
        assert!(root(&e).join("Binaries").join("mod.dll").exists());
        assert!(!root(&e).join("Binaries").join("Tidepool.exe").exists());
        assert!(!root(&e).join("readme.txt").exists());
    }

    #[test]
    fn uninstall_with_a_paused_update_deletes_files_from_both_manifests() {
        use crate::install::manifest::{FileEntry, Manifest};
        let e = installed();
        e.store.finish_install(&e.job, &sample()).unwrap();
        // A paused update whose new version adds one more file, already written.
        let mut files = sample().files.clone();
        files.push(FileEntry { install_tags: vec![], filename: "Content/New.pak".into(), sha1: [0; 20], executable: false, parts: vec![] });
        let s = sample();
        let newer = Manifest::new(s.meta.clone(), s.chunks.clone(), files);
        let update = JobInfo { update_from: Some("1.0.0-CL1".into()), ..e.job.clone() };
        e.store.save_job(&update, &build_binary(&newer, true)).unwrap();
        std::fs::create_dir_all(root(&e).join("Content")).unwrap();
        std::fs::write(root(&e).join("Content").join("New.pak"), b"new").unwrap();
        uninstall(&e.store, "Tide").unwrap();
        assert!(!root(&e).exists(), "both versions' files are gone");
        assert!(e.store.pending_jobs().is_empty(), "the paused update is gone too");
    }

    #[test]
    fn uninstall_unknown_is_not_installed() {
        let e = installed();
        assert_eq!(uninstall(&e.store, "Nope"), Err(InstallError::NotInstalled));
    }

    #[test]
    fn uninstall_after_user_deleted_the_folder_still_forgets_it() {
        let e = installed();
        e.store.finish_install(&e.job, &sample()).unwrap();
        std::fs::remove_dir_all(root(&e)).unwrap();
        uninstall(&e.store, "Tide").unwrap();
        assert!(e.store.installed().is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn locked_file_keeps_record() {
        use std::os::windows::fs::OpenOptionsExt;
        let e = installed();
        e.store.finish_install(&e.job, &sample()).unwrap();
        let _lock = std::fs::OpenOptions::new().read(true).share_mode(0).open(root(&e).join("readme.txt")).unwrap();
        assert_eq!(uninstall(&e.store, "Tide"), Err(InstallError::InUse));
        assert_eq!(e.store.installed().len(), 1);
    }

    #[test]
    fn discard_removes_partial_files_and_job() {
        let e = installed();
        std::fs::remove_file(root(&e).join("readme.txt")).unwrap(); // not downloaded yet
        discard_download(&e.store, &e.job).unwrap();
        assert!(!root(&e).exists());
        assert!(e.store.pending_jobs().is_empty());
    }
}
