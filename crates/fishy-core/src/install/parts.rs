//! Optional parts (spec §6n): which of a manifest's files belong to an install.
use super::manifest::{FileEntry, Manifest};
use super::store::file_path;
use std::collections::BTreeSet;
use std::path::Path;

/// Whether any file has a part name.
pub fn has_parts(m: &Manifest) -> bool {
    m.files.iter().any(|f| !f.install_tags.is_empty())
}

/// A file with no part name always belongs; otherwise any of its names must be chosen.
pub fn belongs(f: &FileEntry, parts: &[String]) -> bool {
    f.install_tags.is_empty() || f.install_tags.iter().any(|t| parts.contains(t))
}

/// The parts already installed: every part name with at least one of its files on disk.
/// None for a manifest without part names.
pub fn chosen_on_disk(root: &Path, m: &Manifest) -> Option<Vec<String>> {
    if !has_parts(m) {
        return None;
    }
    let present = |f: &FileEntry| file_path(root, &f.filename).is_ok_and(|p| p.is_file());
    let chosen: BTreeSet<&String> = m.files.iter().filter(|f| !f.install_tags.is_empty() && present(f)).flat_map(|f| &f.install_tags).collect();
    Some(chosen.into_iter().cloned().collect())
}

/// Only the files that belong. None keeps every file.
pub fn only_parts(mut m: Manifest, parts: Option<&[String]>) -> Manifest {
    if let Some(parts) = parts {
        m.files.retain(|f| belongs(f, parts));
    }
    m
}

/// The parts for an update: the saved ones plus every part name the new version has and the old
/// one didn't (it might be required). Without the old manifest, just the saved ones.
pub fn parts_for_update(saved: &[String], old: Option<&Manifest>, new: &Manifest) -> Vec<String> {
    let mut out: BTreeSet<String> = saved.iter().cloned().collect();
    if let Some(old) = old {
        let before: BTreeSet<&String> = old.files.iter().flat_map(|f| &f.install_tags).collect();
        out.extend(new.files.iter().flat_map(|f| &f.install_tags).filter(|t| !before.contains(t)).cloned());
    }
    out.into_iter().collect()
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
        assert_eq!(parts_for_update(&strings(&["A"]), Some(&old), &new), strings(&["A", "C"]));
    }

    #[test]
    fn update_without_old_manifest_keeps_saved_parts() {
        let new = manifest(vec![file("a", &["A"]), file("c", &["C"])]);
        assert_eq!(parts_for_update(&strings(&["A"]), None, &new), strings(&["A"]));
    }
}
