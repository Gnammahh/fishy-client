//! What a download run will do: which files to write and which chunks to fetch, in order.
use super::manifest::Manifest;
use super::{Guid, InstallError};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadPlan {
    /// Indices into `manifest.files`, sorted by lowercase name, finished files left out.
    pub files: Vec<usize>,
    /// Each chunk once, in the order the files first need it.
    pub chunk_order: Vec<Guid>,
    /// How many parts of the planned files use each chunk (it's kept in memory until the last).
    pub uses: HashMap<Guid, u32>,
    /// Bytes to download (compressed) for this run.
    pub download_size: u64,
    /// Install size of the files already finished in an earlier run.
    pub done_size: u64,
}

pub fn build_plan(m: &Manifest, finished: &HashSet<String>) -> Result<DownloadPlan, InstallError> {
    let mut files: Vec<usize> = (0..m.files.len()).collect();
    files.sort_by_cached_key(|&i| m.files[i].filename.to_lowercase());
    let (done, files): (Vec<usize>, Vec<usize>) = files.into_iter().partition(|&i| finished.contains(&m.files[i].filename));
    let done_size = done.iter().map(|&i| m.files[i].size()).sum();

    let mut chunk_order = Vec::new();
    let mut uses: HashMap<Guid, u32> = HashMap::new();
    let mut download_size = 0;
    for &i in &files {
        for part in &m.files[i].parts {
            let chunk = m.chunk(&part.guid).ok_or_else(|| InstallError::BadManifest("unknown chunk".into()))?;
            let count = uses.entry(part.guid).or_insert(0);
            if *count == 0 {
                chunk_order.push(part.guid);
                download_size += chunk.file_size;
            }
            *count += 1;
        }
    }
    Ok(DownloadPlan { files, chunk_order, uses, download_size, done_size })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::manifest::{ChunkInfo, ChunkPart, FileEntry, ManifestMeta};

    fn chunk(n: u32, size: u64) -> ChunkInfo {
        ChunkInfo { guid: [n, 0, 0, 0], hash: n as u64, sha1: [0; 20], group: 0, window_size: 100, file_size: size }
    }
    fn file(name: &str, chunks: &[u32]) -> FileEntry {
        FileEntry { filename: name.into(), sha1: [0; 20], executable: false, parts: chunks.iter().map(|&c| ChunkPart { guid: [c, 0, 0, 0], offset: 0, size: 10 }).collect() }
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
}
