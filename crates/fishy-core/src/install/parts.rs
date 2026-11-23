//! Optional parts (spec §6n): which of a manifest's files belong to an install.
use super::manifest::{FileEntry, Manifest};
use super::store::file_path;
use super::manifest::parse_manifest;
use super::store::InstallStore;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

/// Whether any file has a part name.
pub fn has_parts(m: &Manifest) -> bool {
    m.files.iter().any(|f| !f.install_tags.is_empty())
}

/// A file with no part name always belongs; otherwise any of its names must be chosen.
pub fn belongs(f: &FileEntry, parts: &[String]) -> bool {
    f.install_tags.is_empty() || f.install_tags.iter().any(|t| parts.contains(t))
}

/// The parts already installed: every part name with at least half of its files on disk (a
/// file shared with another part doesn't choose that part on its own). None for a manifest
/// without part names.
pub fn chosen_on_disk(root: &Path, m: &Manifest) -> Option<Vec<String>> {
    if !has_parts(m) {
        return None;
    }
    let mut count: BTreeMap<&String, (usize, usize)> = BTreeMap::new();
    for f in m.files.iter().filter(|f| !f.install_tags.is_empty()) {
        let here = file_path(root, &f.filename).is_ok_and(|p| p.is_file());
        for t in &f.install_tags {
            let c = count.entry(t).or_default();
            c.0 += usize::from(here);
            c.1 += 1;
        }
    }
    Some(count.into_iter().filter(|(_, (here, all))| here * 2 >= *all).map(|(t, _)| t.clone()).collect())
}

/// Only the files that belong. None keeps every file.
pub fn only_parts(mut m: Manifest, parts: Option<&[String]>) -> Manifest {
    if let Some(parts) = parts {
        m.files.retain(|f| belongs(f, parts));
    }
    m
}

/// The parts for an update: the ones on disk now (`on_disk`, so parts added or removed in Epic's
/// launcher are followed; the saved ones when unknown) plus every part name the new version has
/// and the old one didn't (it might be required). Without the old manifest, no new names.
pub fn parts_for_update(saved: &[String], on_disk: Option<&[String]>, old: Option<&Manifest>, new: &Manifest) -> Vec<String> {
    let mut out: BTreeSet<String> = on_disk.unwrap_or(saved).iter().cloned().collect();
    if let Some(old) = old {
        let before: BTreeSet<&String> = old.files.iter().flat_map(|f| &f.install_tags).collect();
        out.extend(new.files.iter().flat_map(|f| &f.install_tags).filter(|t| !before.contains(t)).cloned());
    }
    out.into_iter().collect()
}

/// The old version's files without those another piece of the same game still lists and the new
/// version dropped, so an update of one piece never deletes a file another piece needs (spec §6n).
pub fn keep_siblings_files(mut old: Manifest, new: &Manifest, siblings: &HashSet<String>) -> Manifest {
    let in_new: HashSet<&str> = new.files.iter().map(|f| f.filename.as_str()).collect();
    old.files.retain(|f| in_new.contains(f.filename.as_str()) || !siblings.contains(&f.filename));
    old
}

/// Every file the other installed pieces of `app`'s game list (the game and its add-on pieces,
/// each by its own parts), from their saved manifests.
pub fn sibling_files(store: &InstallStore, app: &str) -> HashSet<String> {
    let all = store.installed();
    let Some(me) = all.iter().find(|g| g.app_name == app) else { return HashSet::new() };
    let game = me.parent.clone().unwrap_or_else(|| me.app_name.clone());
    all.iter()
        .filter(|g| g.app_name != app && (g.app_name == game || g.parent.as_deref() == Some(game.as_str())))
        .filter_map(|g| store.installed_manifest(&g.app_name).ok().and_then(|b| parse_manifest(&b).ok()).map(|m| only_parts(m, g.parts.as_deref())))
        .flat_map(|m| m.files.into_iter().map(|f| f.filename))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::manifest::{FileEntry, Manifest, ManifestMeta};

    fn file(name: &str, tags: &[&str]) -> FileEntry {
        FileEntry { filename: name.into(), sha1: [0; 20], executable: false, parts: vec![], install_tags: tags.iter().map(|t| t.to_string()).collect() }
    }

    fn manifest(files: Vec<FileEntry>) -> Manifest {
        Manifest::new(ManifestMeta { feature_level: 18, ..Default::default() }, vec![], files)
    }

    fn names(m: &Manifest) -> Vec<&str> {
        m.files.iter().map(|f| f.filename.as_str()).collect()
    }

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn untagged_and_empty_named_files_always_belong() {
        assert!(belongs(&file("a", &[]), &[]));
        assert!(belongs(&file("a", &[]), &strings(&["X"])));
    }

    #[test]
    fn a_file_belongs_if_any_of_its_names_is_chosen() {
        let f = file("a", &["A", "B"]);
        assert!(belongs(&f, &strings(&["B"])));
        assert!(!belongs(&f, &strings(&["C"])));
    }

    #[test]
    fn has_parts_only_with_names() {
        assert!(!has_parts(&manifest(vec![file("a", &[])])));
        assert!(has_parts(&manifest(vec![file("a", &[]), file("b", &["A"])])));
    }

    #[test]
    fn chosen_on_disk_reads_the_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Content")).unwrap();
        std::fs::write(dir.path().join("Content").join("a1.pak"), b"x").unwrap();
        std::fs::write(dir.path().join("base.exe"), b"x").unwrap();
        let m = manifest(vec![
            file("base.exe", &[]),
            file("Content/a1.pak", &["A"]),
            file("Content/a2.pak", &["A"]),
            file("Content/b.pak", &["B"]),
        ]);
        assert_eq!(chosen_on_disk(dir.path(), &m), Some(strings(&["A"])));
        assert_eq!(chosen_on_disk(dir.path(), &manifest(vec![file("base.exe", &[])])), None);
    }

    #[test]
    fn only_parts_none_keeps_everything() {
        let m = manifest(vec![file("a", &[]), file("b", &["B"])]);
        assert_eq!(only_parts(m.clone(), None), m);
    }

    #[test]
    fn only_parts_drops_unchosen_files() {
        let m = manifest(vec![file("a", &[]), file("b", &["B"]), file("c", &["C", "A"])]);
        assert_eq!(names(&only_parts(m, Some(&strings(&["A"])))), vec!["a", "c"]);
    }

    #[test]
    fn update_adds_brand_new_parts() {
        let old = manifest(vec![file("a", &["A"]), file("b", &["B"])]);
        let new = manifest(vec![file("a", &["A"]), file("b", &["B"]), file("c", &["C"])]);
        assert_eq!(parts_for_update(&strings(&["A"]), None, Some(&old), &new), strings(&["A", "C"]));
    }

    #[test]
    fn update_without_old_manifest_keeps_saved_parts() {
        let new = manifest(vec![file("a", &["A"]), file("c", &["C"])]);
        assert_eq!(parts_for_update(&strings(&["A"]), None, None, &new), strings(&["A"]));
    }

    #[test]
    fn update_follows_the_parts_on_disk() {
        // Added B in Epic's launcher, removed A: the files on disk win over what was saved.
        let old = manifest(vec![file("a", &["A"]), file("b", &["B"])]);
        let new = old.clone();
        assert_eq!(parts_for_update(&strings(&["A"]), Some(&strings(&["B"])), Some(&old), &new), strings(&["B"]));
    }

    #[test]
    fn a_shared_file_does_not_choose_the_other_part() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["shared.pak", "a.pak"] {
            std::fs::write(dir.path().join(name), b"x").unwrap();
        }
        let m = manifest(vec![
            file("shared.pak", &["A", "B"]),
            file("a.pak", &["A"]),
            file("b1.pak", &["B"]),
            file("b2.pak", &["B"]),
        ]);
        assert_eq!(chosen_on_disk(dir.path(), &m), Some(strings(&["A"])));
    }

    #[test]
    fn files_another_piece_lists_are_never_removed() {
        let old = manifest(vec![file("keep.pak", &[]), file("moved.pak", &[]), file("gone.pak", &[])]);
        let new = manifest(vec![file("keep.pak", &[])]);
        let siblings: std::collections::HashSet<String> = ["moved.pak".to_string(), "keep.pak".to_string()].into();
        assert_eq!(names(&keep_siblings_files(old, &new, &siblings)), vec!["keep.pak", "gone.pak"]);
    }

    #[test]
    fn sibling_files_lists_the_other_pieces_of_the_game() {
        use crate::install::manifest::testbuild::build_binary;
        use crate::install::store::JobInfo;
        let dir = tempfile::tempdir().unwrap();
        let store = InstallStore::at(dir.path().join("data"));
        let add = |app: &str, parent: Option<&str>, files: Vec<FileEntry>| {
            let m = manifest(files);
            let job = JobInfo {
                app_name: app.into(),
                title: app.into(),
                namespace: "ns".into(),
                catalog_item_id: "cat".into(),
                install_path: dir.path().join("Fortnite"),
                download_size: 0,
                install_size: 0,
                base_urls: vec![],
                update_from: None,
                imported: true,
                parts: Some(vec!["A".into()]),
                parent: parent.map(str::to_string),
            };
            store.save_job(&job, &build_binary(&m, true)).unwrap();
            store.finish_install(&job, &m).unwrap();
        };
        add("Fortnite", None, vec![file("game.pak", &[]), file("stw.pak", &["A"]), file("hd.pak", &["B"])]);
        add("stw", Some("Fortnite"), vec![file("stw.pak", &["A"])]);
        add("lego", Some("Fortnite"), vec![file("lego.pak", &[])]);
        add("Other", None, vec![file("other.pak", &[])]);
        let mut got: Vec<String> = sibling_files(&store, "stw").into_iter().collect();
        got.sort();
        assert_eq!(got, strings(&["game.pak", "lego.pak", "stw.pak"]), "only the same game's pieces, by their parts");
    }

    #[test]
    fn an_update_plan_only_touches_the_chosen_parts() {
        use crate::install::plan::build_update_plan;
        let old = manifest(vec![file("base", &[]), file("a", &["A"]), file("b", &["B"]), file("b_old", &["B"])]);
        let new = manifest(vec![file("base", &[]), file("a", &["A"]), file("b", &["B"]), file("a2", &["A"])]);
        let chosen = strings(&["A"]);
        let plan = build_update_plan(&only_parts(new.clone(), Some(&chosen)), Some(&only_parts(old, Some(&chosen))), &Default::default()).unwrap();
        let new_a = only_parts(new, Some(&chosen));
        let written: Vec<&str> = plan.files.iter().map(|&i| new_a.files[i].filename.as_str()).collect();
        assert!(!written.contains(&"b"), "an unchosen part is never downloaded");
        assert!(plan.removed.is_empty(), "b_old belongs to an unchosen part, so it's not deleted");
    }
}
