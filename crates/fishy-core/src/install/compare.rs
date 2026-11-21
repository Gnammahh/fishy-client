//! Which files an update adds, changes, keeps or removes (Legendary `ManifestComparison`, spec §6e).
use super::manifest::Manifest;
use std::collections::{BTreeSet, HashMap};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Comparison {
    pub added: BTreeSet<String>,
    pub changed: BTreeSet<String>,
    pub unchanged: BTreeSet<String>,
    pub removed: BTreeSet<String>,
}

/// Files are matched by name; their SHA-1 decides changed or unchanged.
pub fn compare(old: &Manifest, new: &Manifest) -> Comparison {
    let mut old_files: HashMap<&str, [u8; 20]> = old.files.iter().map(|f| (f.filename.as_str(), f.sha1)).collect();
    let mut c = Comparison::default();
    for f in &new.files {
        match old_files.remove(f.filename.as_str()) {
            Some(sha) if sha == f.sha1 => c.unchanged.insert(f.filename.clone()),
            Some(_) => c.changed.insert(f.filename.clone()),
            None => c.added.insert(f.filename.clone()),
        };
    }
    c.removed = old_files.into_keys().map(str::to_string).collect();
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::manifest::{FileEntry, ManifestMeta};

    fn m(files: &[(&str, u8)]) -> Manifest {
        let files = files.iter().map(|(n, s)| FileEntry { filename: n.to_string(), sha1: [*s; 20], executable: false, parts: vec![] }).collect();
        Manifest::new(ManifestMeta::default(), vec![], files)
    }

    #[test]
    fn compare_sorts_files_into_four_groups() {
        let old = m(&[("keep", 1), ("edit", 2), ("gone", 3)]);
        let new = m(&[("keep", 1), ("edit", 9), ("fresh", 4)]);
        let c = compare(&old, &new);
        let set = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<BTreeSet<_>>();
        assert_eq!(c.unchanged, set(&["keep"]));
        assert_eq!(c.changed, set(&["edit"]));
        assert_eq!(c.added, set(&["fresh"]));
        assert_eq!(c.removed, set(&["gone"]));
    }
}
