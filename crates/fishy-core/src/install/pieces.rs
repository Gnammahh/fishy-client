//! Removing a game's add-on piece, such as Fortnite's LEGO Fortnite content (spec §6u).
use super::manifest::{parse_manifest, FileEntry, Manifest};
use super::parts::{belongs, only_parts};
use super::store::InstallStore;
use super::uninstall::{delete_aside, delete_named_files};
use super::InstallError;
use std::collections::{BTreeSet, HashSet};

/// What removing a piece deletes, and the game's chosen parts afterwards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PieceRemoval {
    /// File names to delete, sorted.
    pub delete: Vec<String>,
    pub game_parts: Option<Vec<String>>,
}

/// The piece's part names come out of the game's chosen parts, but only a name every game file
/// of which is one of the piece's own files (so a shared name, a language say, is never taken
/// away). The files deleted: the piece's own and the game's that belonged only through those
/// names, never one the game still lists or `others` (the other pieces' files) lists.
pub fn piece_removal(game: &Manifest, game_parts: Option<&[String]>, piece: &Manifest, piece_parts: Option<&[String]>, others: &HashSet<String>) -> PieceRemoval {
    let in_piece = |f: &&FileEntry| piece_parts.is_none_or(|p| belongs(f, p));
    let piece_files: Vec<&FileEntry> = piece.files.iter().filter(in_piece).collect();
    let piece_names: HashSet<&str> = piece_files.iter().map(|f| f.filename.as_str()).collect();
    let removable: BTreeSet<&String> = piece_files
        .iter()
        .flat_map(|f| &f.install_tags)
        .filter(|t| game.files.iter().filter(|f| f.install_tags.contains(t)).all(|f| piece_names.contains(f.filename.as_str())))
        .collect();
    let new_parts: Option<Vec<String>> = game_parts.map(|p| p.iter().filter(|t| !removable.contains(t)).cloned().collect());
    let listed = |parts: Option<&[String]>| -> HashSet<&str> { game.files.iter().filter(|f| parts.is_none_or(|p| belongs(f, p))).map(|f| f.filename.as_str()).collect() };
    let before = listed(game_parts);
    let after = listed(new_parts.as_deref());
    let delete: BTreeSet<String> = piece_names
        .iter()
        .copied()
        .chain(before.difference(&after).copied())
        .filter(|n| !after.contains(n) && !others.contains(*n))
        .map(str::to_string)
        .collect();
    PieceRemoval { delete: delete.into_iter().collect(), game_parts: new_parts }
}

/// What `remove_piece` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Removed {
    pub game: String,
    pub files: usize,
    /// The part names taken out of the game's chosen parts.
    pub part_names: Vec<String>,
}

fn saved_manifest(store: &InstallStore, app: &str, parts: Option<&[String]>) -> Result<Manifest, InstallError> {
    let bytes = store.installed_manifest(app).map_err(|e| InstallError::Io { folder: String::new(), detail: e.to_string() })?;
    Ok(only_parts(parse_manifest(&bytes)?, parts))
}

/// Removes an installed add-on piece (spec §6u): its files, its record, its waiting update; the
/// game's chosen parts and waiting update lose its part names. Nothing is forgotten if a file
/// can't be deleted. The caller makes sure nothing of the game is downloading or running.
pub fn remove_piece(store: &InstallStore, app: &str) -> Result<Removed, InstallError> {
    drop_piece(store, app, true)
}

/// Forgets a piece Epic's launcher removed (spec §6u): the same records change as
/// [`remove_piece`], but no game file is touched (Epic already deleted them).
pub fn forget_piece(store: &InstallStore, app: &str) -> Result<Removed, InstallError> {
    drop_piece(store, app, false)
}

fn drop_piece(store: &InstallStore, app: &str, delete_files: bool) -> Result<Removed, InstallError> {
    let all = store.installed();
    let piece = all.iter().find(|g| g.app_name == app && g.parent.is_some()).ok_or(InstallError::NotInstalled)?;
    let game_name = piece.parent.clone().unwrap_or_default();
    let game = all.iter().find(|g| g.app_name == game_name).ok_or(InstallError::NotInstalled)?;
    let game_m = saved_manifest(store, &game.app_name, None)?;
    let piece_m = saved_manifest(store, app, None)?;
    let others: HashSet<String> = all
        .iter()
        .filter(|g| g.parent.as_deref() == Some(game_name.as_str()) && g.app_name != app)
        .filter_map(|g| saved_manifest(store, &g.app_name, g.parts.as_deref()).ok())
        .flat_map(|m| m.files.into_iter().map(|f| f.filename))
        .collect();
    let plan = piece_removal(&game_m, game.parts.as_deref(), &piece_m, piece.parts.as_deref(), &others);
    let part_names: Vec<String> = game.parts.iter().flatten().filter(|t| !plan.game_parts.iter().flatten().any(|n| n == *t)).cloned().collect();
    let root = &piece.install_path;
    // The piece's half-built update is Fishy's own; it goes either way.
    if !delete_aside(root, app) || (delete_files && !delete_named_files(root, plan.delete.iter().map(String::as_str), false)) {
        return Err(InstallError::InUse);
    }
    let io = |e: std::io::Error| InstallError::Io { folder: root.display().to_string(), detail: e.to_string() };
    store.remove_job(app).map_err(io)?;
    store.forget_installed(app).map_err(io)?;
    store.set_parts(&game_name, plan.game_parts.clone()).map_err(io)?;
    // The game's waiting update no longer fetches the piece's part names either.
    if let Some(mut job) = store.pending_jobs().into_iter().find(|j| j.app_name == game_name)
        && let Some(p) = job.parts.as_mut() {
            p.retain(|t| !part_names.contains(t));
            let bytes = store.job_manifest(&game_name).map_err(io)?;
            store.save_job(&job, &bytes).map_err(io)?;
        }
    let files = if delete_files { plan.delete.len() } else { 0 };
    if delete_files {
        tracing::info!("piece removed ({files} files deleted)");
    } else {
        tracing::info!("piece removed in Epic's launcher: record forgotten");
    }
    Ok(Removed { game: game_name, files, part_names })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::manifest::testbuild::{build_binary, sample};
    use crate::install::manifest::{ChunkPart, FileEntry, Manifest};
    use crate::install::store::{file_path, InstallStore, JobInfo};
    use std::collections::HashSet;
    use std::path::Path;

    fn file(name: &str, tags: &[&str]) -> FileEntry {
        let c1 = [1, 2, 3, 4];
        FileEntry { install_tags: tags.iter().map(|t| t.to_string()).collect(), filename: name.into(), sha1: [3; 20], executable: false, parts: vec![ChunkPart { guid: c1, offset: 0, size: 10 }] }
    }

    fn manifest(files: Vec<FileEntry>) -> Manifest {
        let s = sample();
        Manifest::new(s.meta, s.chunks, files)
    }

    /// Fortnite in small: the game lists every file; LEGO and Save the World list theirs.
    fn game() -> Manifest {
        manifest(vec![
            file("FortniteGame/Binaries/Fortnite.exe", &[]),
            file("Paks/br.pak", &["FortniteBR"]),
            file("Paks/juno1.pak", &["GFP_JunoRoot"]),
            file("Paks/juno2.pak", &["GFP_JunoRoot"]),
            file("Paks/stw.pak", &["GFP_SaveTheWorldRoot"]),
            file("Paks/de.pak", &["Langde"]),
        ])
    }

    fn lego() -> Manifest {
        manifest(vec![file("Paks/juno1.pak", &["GFP_JunoRoot"]), file("Paks/juno2.pak", &["GFP_JunoRoot"])])
    }

    fn parts(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    fn names(v: &[&str]) -> Vec<String> {
        let mut out: Vec<String> = v.iter().map(|n| n.to_string()).collect();
        out.sort();
        out
    }

    #[test]
    fn removing_lego_deletes_its_files_and_takes_its_part_out_of_the_game() {
        let chosen = parts(&["FortniteBR", "GFP_JunoRoot", "GFP_SaveTheWorldRoot", "Langde"]);
        let stw: HashSet<String> = HashSet::from(["Paks/stw.pak".to_string()]);
        let r = piece_removal(&game(), Some(&chosen), &lego(), Some(&parts(&["GFP_JunoRoot"])), &stw);
        assert_eq!(r.delete, names(&["Paks/juno1.pak", "Paks/juno2.pak"]));
        assert_eq!(r.game_parts, Some(parts(&["FortniteBR", "GFP_SaveTheWorldRoot", "Langde"])));
    }

    #[test]
    fn a_part_name_the_game_also_uses_elsewhere_is_never_taken_out() {
        // A piece that also lists a German file the game needs for itself.
        let piece = manifest([lego().files, vec![file("Paks/de.pak", &["Langde"])]].concat());
        let game = manifest([game().files, vec![file("Paks/de2.pak", &["Langde"])]].concat());
        let chosen = parts(&["GFP_JunoRoot", "Langde"]);
        let r = piece_removal(&game, Some(&chosen), &piece, None, &HashSet::new());
        assert_eq!(r.delete, names(&["Paks/juno1.pak", "Paks/juno2.pak"]), "the German file stays: the game still lists it");
        assert_eq!(r.game_parts, Some(parts(&["Langde"])));
    }

    #[test]
    fn a_file_another_piece_still_lists_is_kept() {
        let chosen = parts(&["GFP_JunoRoot"]);
        let other: HashSet<String> = HashSet::from(["Paks/juno2.pak".to_string()]);
        let r = piece_removal(&game(), Some(&chosen), &lego(), None, &other);
        assert_eq!(r.delete, names(&["Paks/juno1.pak"]));
    }

    struct Env {
        _dir: tempfile::TempDir,
        store: InstallStore,
        root: std::path::PathBuf,
    }

    fn job(app: &str, root: &Path, parts: &[&str], parent: Option<&str>) -> JobInfo {
        JobInfo {
            app_name: app.into(),
            title: format!("Title {app}"),
            namespace: "ns".into(),
            catalog_item_id: "c".into(),
            install_path: root.to_path_buf(),
            download_size: 1,
            install_size: 1,
            base_urls: vec![],
            update_from: None,
            imported: true,
            parts: Some(self::parts(parts)),
            parent: parent.map(str::to_string),
        }
    }

    fn installed(app: &str, root: &Path, store: &InstallStore, m: &Manifest, parts: &[&str], parent: Option<&str>) {
        let j = job(app, root, parts, parent);
        store.save_job(&j, &build_binary(m, true)).unwrap();
        store.finish_install(&j, m).unwrap();
    }

    /// Fortnite with LEGO and Save the World installed, every file on disk.
    fn fortnite() -> Env {
        let dir = tempfile::tempdir().unwrap();
        let store = InstallStore::at(dir.path().join("data"));
        let root = dir.path().join("Fortnite");
        for f in &game().files {
            let p = file_path(&root, &f.filename).unwrap();
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b"x").unwrap();
        }
        let stw = manifest(vec![file("Paks/stw.pak", &["GFP_SaveTheWorldRoot"])]);
        installed("Fortnite", &root, &store, &game(), &["FortniteBR", "GFP_JunoRoot", "GFP_SaveTheWorldRoot", "Langde"], None);
        installed("Lego", &root, &store, &lego(), &["GFP_JunoRoot"], Some("Fortnite"));
        installed("Stw", &root, &store, &stw, &["GFP_SaveTheWorldRoot"], Some("Fortnite"));
        Env { _dir: dir, store, root }
    }

    #[test]
    fn remove_piece_deletes_lego_and_keeps_fortnite_and_save_the_world() {
        let e = fortnite();
        let removed = remove_piece(&e.store, "Lego").unwrap();
        assert_eq!(removed.files, 2);
        assert_eq!(removed.game, "Fortnite");
        assert_eq!(removed.part_names, parts(&["GFP_JunoRoot"]));
        assert!(!e.root.join("Paks/juno1.pak").exists() && !e.root.join("Paks/juno2.pak").exists());
        for kept in ["FortniteGame/Binaries/Fortnite.exe", "Paks/br.pak", "Paks/stw.pak", "Paks/de.pak"] {
            assert!(e.root.join(kept).is_file(), "{kept} kept");
        }
        let all = e.store.installed();
        assert!(all.iter().all(|g| g.app_name != "Lego"), "the piece's record is gone");
        assert!(e.store.installed_manifest("Lego").is_err());
        let fortnite = all.iter().find(|g| g.app_name == "Fortnite").unwrap();
        assert_eq!(fortnite.parts, Some(parts(&["FortniteBR", "GFP_SaveTheWorldRoot", "Langde"])));
    }

    #[test]
    fn a_paused_update_of_the_piece_goes_and_the_games_is_narrowed() {
        let e = fortnite();
        let mut lego_update = job("Lego", &e.root, &["GFP_JunoRoot"], Some("Fortnite"));
        lego_update.update_from = Some("1".into());
        e.store.save_job(&lego_update, &build_binary(&lego(), true)).unwrap();
        let aside = crate::install::uninstall::aside_dir(&e.root, "Lego");
        std::fs::create_dir_all(&aside).unwrap();
        std::fs::write(aside.join("half.pak"), b"x").unwrap();
        let mut game_update = job("Fortnite", &e.root, &["FortniteBR", "GFP_JunoRoot", "GFP_SaveTheWorldRoot", "Langde", "NewPart"], None);
        game_update.update_from = Some("1".into());
        e.store.save_job(&game_update, &build_binary(&game(), true)).unwrap();

        remove_piece(&e.store, "Lego").unwrap();
        let jobs = e.store.pending_jobs();
        assert_eq!(jobs.iter().map(|j| j.app_name.as_str()).collect::<Vec<_>>(), vec!["Fortnite"]);
        assert_eq!(jobs[0].parts, Some(parts(&["FortniteBR", "GFP_SaveTheWorldRoot", "Langde", "NewPart"])));
        assert!(!e.root.join(".fishy-update").exists());
    }

    #[test]
    fn forgetting_a_piece_epic_removed_keeps_every_file_and_narrows_the_game() {
        // Epic's launcher already deleted the piece; Fishy Client only drops its own record so
        // its next update doesn't fetch the piece again.
        let e = fortnite();
        let forgot = forget_piece(&e.store, "Lego").unwrap();
        assert_eq!((forgot.game.as_str(), forgot.files), ("Fortnite", 0));
        assert_eq!(forgot.part_names, parts(&["GFP_JunoRoot"]));
        for f in &game().files {
            assert!(file_path(&e.root, &f.filename).unwrap().is_file(), "{} untouched", f.filename);
        }
        let all = e.store.installed();
        assert!(all.iter().all(|g| g.app_name != "Lego"));
        let fortnite = all.iter().find(|g| g.app_name == "Fortnite").unwrap();
        assert_eq!(fortnite.parts, Some(parts(&["FortniteBR", "GFP_SaveTheWorldRoot", "Langde"])));
    }

    #[test]
    fn only_an_installed_piece_can_be_removed() {
        let e = fortnite();
        assert_eq!(remove_piece(&e.store, "Fortnite"), Err(crate::install::InstallError::NotInstalled), "the game itself is Uninstall");
        assert_eq!(remove_piece(&e.store, "Nope"), Err(crate::install::InstallError::NotInstalled));
    }
}
