//! What a download run will do: which files to write and which chunks to fetch, in order.
//! For an update (spec §6e) it also says which parts are copied from the old files.
use super::compare::compare;
use super::manifest::Manifest;
use super::{Guid, InstallError};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadPlan {
    /// Indices into `manifest.files`, sorted by lowercase name. Finished and unchanged files are left out.
    pub files: Vec<usize>,
    /// Each downloaded chunk once, in the order the files first need it.
    pub chunk_order: Vec<Guid>,
    /// How many downloaded parts use each chunk (it's kept in memory until the last).
    pub uses: HashMap<Guid, u32>,
    /// Bytes to download (compressed) for this run.
    pub download_size: u64,
    /// Install size of files that need no writing (finished earlier, or unchanged by an update).
    pub done_size: u64,
    /// Changed files only: one entry per part, `Some(position in the old file)` when copied from it.
    pub reuse: HashMap<usize, Vec<Option<u64>>>,
    /// Changed files: written to a temporary file, then swapped over the old one.
    pub replace: HashSet<usize>,
    /// Files the old version had and the new one doesn't; deleted at the end.
    pub removed: Vec<String>,
    /// Free space this run needs.
    pub space_needed: u64,
    /// Free space it needs when the update is built aside (spec §6u): every changed file whole.
    pub aside_space: u64,
}

pub fn build_plan(m: &Manifest, finished: &HashSet<String>) -> Result<DownloadPlan, InstallError> {
    build_update_plan(m, None, finished)
}

/// Where each part of a changed file can be read from the old file, if anywhere
/// (Legendary `run_analysis`: the new part must lie wholly inside an old part with the same chunk).
fn reusable_parts(old: &super::manifest::FileEntry, new: &super::manifest::FileEntry) -> Vec<Option<u64>> {
    let mut by_chunk: HashMap<Guid, Vec<(u64, u32, u32)>> = HashMap::new();
    let mut pos = 0u64;
    for p in &old.parts {
        by_chunk.entry(p.guid).or_default().push((pos, p.offset, p.offset + p.size));
        pos += p.size as u64;
    }
    new.parts
        .iter()
        .map(|p| {
            by_chunk.get(&p.guid)?.iter().find(|&&(_, start, end)| start <= p.offset && p.offset + p.size <= end).map(|&(file_pos, start, _)| file_pos + (p.offset - start) as u64)
        })
        .collect()
}

pub fn build_update_plan(m: &Manifest, old: Option<&Manifest>, finished: &HashSet<String>) -> Result<DownloadPlan, InstallError> {
    let comparison = old.map(|o| compare(o, m));
    let skip = |name: &str| finished.contains(name) || comparison.as_ref().is_some_and(|c| c.unchanged.contains(name));
    let mut files: Vec<usize> = (0..m.files.len()).collect();
    files.sort_by_cached_key(|&i| m.files[i].filename.to_lowercase());
    let (done, files): (Vec<usize>, Vec<usize>) = files.into_iter().partition(|&i| skip(&m.files[i].filename));
    let done_size = done.iter().map(|&i| m.files[i].size()).sum();

    let old_files: HashMap<&str, &super::manifest::FileEntry> =
        old.map(|o| o.files.iter().map(|f| (f.filename.as_str(), f)).collect()).unwrap_or_default();
    let mut reuse = HashMap::new();
    let mut replace = HashSet::new();
    let (mut added_size, mut growth, mut largest_changed, mut changed_size) = (0u64, 0u64, 0u64, 0u64);
    for &i in &files {
        let entry = &m.files[i];
        match old_files.get(entry.filename.as_str()) {
            Some(old_entry) => {
                reuse.insert(i, reusable_parts(old_entry, entry));
                replace.insert(i);
                growth += entry.size().saturating_sub(old_entry.size());
                largest_changed = largest_changed.max(entry.size());
                changed_size += entry.size();
            }
            None => added_size += entry.size(),
        }
    }

    let mut chunk_order = Vec::new();
    let mut uses: HashMap<Guid, u32> = HashMap::new();
    let mut download_size = 0;
    for &i in &files {
        for (n, part) in m.files[i].parts.iter().enumerate() {
            if reuse.get(&i).is_some_and(|r: &Vec<Option<u64>>| r[n].is_some()) {
                continue;
            }
            let chunk = m.chunk(&part.guid).ok_or_else(|| InstallError::BadManifest("unknown chunk".into()))?;
            let count = uses.entry(part.guid).or_insert(0);
            if *count == 0 {
                chunk_order.push(part.guid);
                download_size += chunk.file_size;
            }
            *count += 1;
        }
    }
    let removed = comparison.map(|c| c.removed.into_iter().collect()).unwrap_or_default();
    let space_needed = added_size + growth + largest_changed;
    let aside_space = added_size + changed_size;
    Ok(DownloadPlan { files, chunk_order, uses, download_size, done_size, reuse, replace, removed, space_needed, aside_space })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::manifest::{ChunkInfo, ChunkPart, FileEntry, ManifestMeta};

    fn chunk(n: u32, size: u64) -> ChunkInfo {
        ChunkInfo { guid: [n, 0, 0, 0], hash: n as u64, sha1: [0; 20], group: 0, window_size: 100, file_size: size }
    }
    fn file(name: &str, chunks: &[u32]) -> FileEntry {
        FileEntry { install_tags: vec![], filename: name.into(), sha1: [0; 20], executable: false, parts: chunks.iter().map(|&c| ChunkPart { guid: [c, 0, 0, 0], offset: 0, size: 10 }).collect() }
    }
    fn manifest(files: Vec<FileEntry>) -> Manifest {
        Manifest::new(ManifestMeta::default(), vec![chunk(1, 50), chunk(2, 60), chunk(3, 70)], files)
    }
    fn none() -> HashSet<String> {
        HashSet::new()
    }

    #[test]
    fn orders_files_case_insensitively() {
        let m = manifest(vec![file("B.txt", &[1]), file("a.txt", &[1]), file("C.txt", &[1])]);
        let plan = build_plan(&m, &none()).unwrap();
        let names: Vec<&str> = plan.files.iter().map(|&i| m.files[i].filename.as_str()).collect();
        assert_eq!(names, vec!["a.txt", "B.txt", "C.txt"]);
    }

    #[test]
    fn chunk_order_is_first_use_and_unique() {
        let m = manifest(vec![file("b", &[1, 3]), file("a", &[2, 1]), file("c", &[3])]);
        let plan = build_plan(&m, &none()).unwrap();
        assert_eq!(plan.chunk_order, vec![[2, 0, 0, 0], [1, 0, 0, 0], [3, 0, 0, 0]]);
        assert_eq!(plan.download_size, 50 + 60 + 70);
    }

    #[test]
    fn uses_count_every_part() {
        let m = manifest(vec![file("a", &[1, 1, 2]), file("b", &[1])]);
        let plan = build_plan(&m, &none()).unwrap();
        assert_eq!(plan.uses[&[1, 0, 0, 0]], 3);
        assert_eq!(plan.uses[&[2, 0, 0, 0]], 1);
        assert!(!plan.uses.contains_key(&[3, 0, 0, 0]));
    }

    #[test]
    fn finished_files_are_skipped_and_sizes_shrink() {
        let m = manifest(vec![file("a", &[1]), file("b", &[2, 1])]);
        let plan = build_plan(&m, &HashSet::from(["a".to_string()])).unwrap();
        assert_eq!(plan.files, vec![1]);
        assert_eq!(plan.done_size, 10);
        assert_eq!(plan.uses[&[1, 0, 0, 0]], 1);
        assert_eq!(plan.download_size, 60 + 50);
        let all_done = build_plan(&m, &HashSet::from(["a".to_string(), "b".to_string()])).unwrap();
        assert!(all_done.files.is_empty() && all_done.chunk_order.is_empty());
        assert_eq!((all_done.download_size, all_done.done_size), (0, 30));
    }

    #[test]
    fn unknown_chunk_is_bad_manifest() {
        let m = manifest(vec![file("a", &[9])]);
        assert!(matches!(build_plan(&m, &none()), Err(InstallError::BadManifest(_))));
    }

    #[test]
    fn empty_file_needs_no_chunks() {
        let m = manifest(vec![file("empty", &[])]);
        let plan = build_plan(&m, &none()).unwrap();
        assert_eq!(plan.files, vec![0]);
        assert!(plan.chunk_order.is_empty());
    }
    /// A file with explicit parts (chunk, offset, size) and a fingerprint byte.
    fn pfile(name: &str, sha: u8, parts: &[(u32, u32, u32)]) -> FileEntry {
        FileEntry { install_tags: vec![],
            filename: name.into(),
            sha1: [sha; 20],
            executable: false,
            parts: parts.iter().map(|&(c, offset, size)| ChunkPart { guid: [c, 0, 0, 0], offset, size }).collect(),
        }
    }

    #[test]
    fn update_plan_skips_unchanged_and_reuses_covered_parts() {
        let old = manifest(vec![pfile("same", 1, &[(3, 0, 10)]), pfile("big", 2, &[(2, 0, 40), (1, 0, 100)])]);
        // "big" changed: (1, 20..50) sits inside the old (1, 0..100) part, which starts at file position 40.
        let new = manifest(vec![pfile("same", 1, &[(3, 0, 10)]), pfile("big", 9, &[(1, 20, 30), (2, 0, 40)])]);
        let plan = build_update_plan(&new, Some(&old), &none()).unwrap();
        assert_eq!(plan.files, vec![1]); // "big" only
        assert_eq!(plan.reuse[&1], vec![Some(40 + 20), Some(0)]);
        assert_eq!(plan.replace, HashSet::from([1]));
        assert!(plan.chunk_order.is_empty() && plan.uses.is_empty());
        assert_eq!(plan.download_size, 0);
        assert_eq!(plan.done_size, 10); // the unchanged file counts as done
    }

    #[test]
    fn a_partly_covered_part_is_downloaded() {
        let old = manifest(vec![pfile("f", 1, &[(1, 0, 10)])]);
        let new = manifest(vec![pfile("f", 2, &[(1, 5, 10)])]); // 5..15 runs past the old 0..10
        let plan = build_update_plan(&new, Some(&old), &none()).unwrap();
        assert_eq!(plan.reuse[&0], vec![None]);
        assert_eq!(plan.chunk_order, vec![[1, 0, 0, 0]]);
        assert_eq!(plan.uses[&[1, 0, 0, 0]], 1);
    }

    #[test]
    fn update_download_size_counts_only_downloaded_chunks() {
        let old = manifest(vec![pfile("f", 1, &[(1, 0, 10)])]);
        let new = manifest(vec![pfile("f", 2, &[(1, 0, 10), (2, 0, 10)]), pfile("added", 3, &[(3, 0, 10)])]);
        let plan = build_update_plan(&new, Some(&old), &none()).unwrap();
        assert_eq!(plan.download_size, 60 + 70);
        assert_eq!(plan.replace.len(), 1);
        let f = new.files.iter().position(|f| f.filename == "f").unwrap();
        assert_eq!(plan.reuse[&f], vec![Some(0), None]);
    }

    #[test]
    fn update_plan_lists_removed_files_and_skips_finished_ones() {
        let old = manifest(vec![pfile("a", 1, &[(1, 0, 10)]), pfile("gone", 1, &[(1, 0, 10)])]);
        let new = manifest(vec![pfile("a", 2, &[(2, 0, 10)]), pfile("b", 3, &[(3, 0, 10)])]);
        let plan = build_update_plan(&new, Some(&old), &HashSet::from(["a".to_string()])).unwrap();
        let names: Vec<&str> = plan.files.iter().map(|&i| new.files[i].filename.as_str()).collect();
        assert_eq!(names, vec!["b"]);
        assert_eq!(plan.removed, vec!["gone".to_string()]);
        assert_eq!(plan.download_size, 70);
    }

    #[test]
    fn update_space_is_added_plus_growth_plus_largest_changed() {
        let old = manifest(vec![pfile("grow", 1, &[(1, 0, 10)]), pfile("shrink", 1, &[(1, 0, 30)])]);
        let new = manifest(vec![
            pfile("grow", 2, &[(1, 0, 25)]),   // +15, temp copy 25
            pfile("shrink", 2, &[(1, 0, 20)]), // +0, temp copy 20
            pfile("added", 3, &[(2, 0, 40)]),  // +40
        ]);
        let plan = build_update_plan(&new, Some(&old), &none()).unwrap();
        assert_eq!(plan.space_needed, 40 + 15 + 25);
    }

    #[test]
    fn install_space_is_what_is_left_to_write() {
        let m = manifest(vec![file("a", &[1]), file("b", &[2, 1])]);
        assert_eq!(build_plan(&m, &none()).unwrap().space_needed, 30);
        assert_eq!(build_plan(&m, &HashSet::from(["a".to_string()])).unwrap().space_needed, 20);
    }
}
