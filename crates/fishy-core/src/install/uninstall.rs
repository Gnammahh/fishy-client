//! Removing games and cancelled downloads. Only files the manifest lists are deleted, then folders
//! left empty (deepest first), so anything the user added to the game folder stays.
use super::manifest::{parse_manifest, Manifest};
use super::store::{file_path, InstallStore, JobInfo};
use super::InstallError;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Deletes the manifest's files under `root` and the folders left empty. False if a file that
/// exists couldn't be deleted (for example, the game is running).
pub fn delete_manifest_files(root: &Path, m: &Manifest) -> bool {
    let mut ok = true;
    let mut dirs: BTreeSet<PathBuf> = BTreeSet::new();
    for f in &m.files {
        // Names that would escape the folder were never written, so there's nothing to delete.
        let Ok(path) = file_path(root, &f.filename) else { continue };
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
    for d in dirs.iter().map(PathBuf::as_path).chain([root]) {
        let _ = std::fs::remove_dir(d);
    }
    ok
}

pub fn uninstall(store: &InstallStore, app: &str) -> Result<(), InstallError> {
    let game = store.installed().into_iter().find(|g| g.app_name == app).ok_or(InstallError::NotInstalled)?;
    let io_err = |e: std::io::Error| InstallError::Io { folder: game.install_path.display().to_string(), detail: e.to_string() };
    // No saved manifest (damaged data folder): forget the record; the files stay for the user to delete.
    if let Some(manifest) = store.installed_manifest(app).ok().and_then(|b| parse_manifest(&b).ok()) {
        if !delete_manifest_files(&game.install_path, &manifest) {
            return Err(InstallError::InUse);
        }
    }
    store.forget_installed(app).map_err(io_err)
}

/// Cancel: deletes what a download wrote so far and its saved progress.
pub fn discard_download(store: &InstallStore, job: &JobInfo) -> Result<(), InstallError> {
    if let Some(manifest) = store.job_manifest(&job.app_name).ok().and_then(|b| parse_manifest(&b).ok()) {
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
