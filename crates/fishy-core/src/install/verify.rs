//! Verify files (spec §6t): every file of an installed game is compared with the manifest's SHA-1.
use super::manifest::{FileEntry, Manifest};
use super::store::file_path;
use crate::epic_import::FileCheck;
use sha1::{Digest, Sha1};
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// Bytes read at a time while hashing.
const BLOCK: usize = 1024 * 1024;

/// Checks the manifest's files under `root`; `None` when `stop` was set. The caller passes a
/// manifest already filtered to the installed parts. `on_progress(done, total)` after each file.
/// One file at a time, on the caller's thread, so it stays light.
pub fn verify(root: &Path, m: &Manifest, stop: &AtomicBool, on_progress: &mut dyn FnMut(usize, usize)) -> Option<FileCheck> {
    let mut check = FileCheck::default();
    let mut buf = vec![0u8; BLOCK];
    let total = m.files.len();
    for (i, f) in m.files.iter().enumerate() {
        if stop.load(Ordering::Relaxed) {
            return None;
        }
        match file_ok(root, f, &mut buf, stop) {
            None => return None,
            Some(true) => check.good.push((hex::encode(f.sha1), f.filename.clone())),
            Some(false) => {
                check.bad_files += 1;
                check.bad_bytes += f.size();
            }
        }
        on_progress(i + 1, total);
    }
    Some(check)
}

/// Some(true): there with the right size and SHA-1. None: stopped while reading.
fn file_ok(root: &Path, f: &FileEntry, buf: &mut [u8], stop: &AtomicBool) -> Option<bool> {
    let Ok(path) = file_path(root, &f.filename) else { return Some(false) };
    // Missing or the wrong size is bad without reading it.
    match std::fs::metadata(&path) {
        Ok(md) if md.is_file() && md.len() == f.size() => {}
        _ => return Some(false),
    }
    let Ok(mut file) = std::fs::File::open(&path) else { return Some(false) };
    let mut hasher = Sha1::new();
    loop {
        if stop.load(Ordering::Relaxed) {
            return None;
        }
        match file.read(buf) {
            Ok(0) => break,
            Ok(n) => hasher.update(&buf[..n]),
            Err(_) => return Some(false),
        }
    }
    Some(<[u8; 20]>::from(hasher.finalize()) == f.sha1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::manifest::{ChunkPart, FileEntry, ManifestMeta};
    use sha1::{Digest, Sha1};
    use std::sync::atomic::Ordering;

    const FILES: [(&str, &[u8]); 3] = [("Bin/game.exe", b"MZ fish game"), ("data/a.pak", b"aaaaaaaaaaaaaaaa"), ("empty.cfg", b"")];

    /// A manifest whose sizes and SHA-1s match FILES, written under a temp folder.
    fn game() -> (tempfile::TempDir, Manifest) {
        let dir = tempfile::tempdir().unwrap();
        let files = FILES
            .iter()
            .map(|(name, bytes)| {
                let p = dir.path().join(name);
                std::fs::create_dir_all(p.parent().unwrap()).unwrap();
                std::fs::write(&p, bytes).unwrap();
                let parts = if bytes.is_empty() { vec![] } else { vec![ChunkPart { guid: [1, 0, 0, 0], offset: 0, size: bytes.len() as u32 }] };
                FileEntry { filename: name.to_string(), sha1: Sha1::digest(bytes).into(), executable: false, parts, install_tags: vec![] }
            })
            .collect();
        (dir, Manifest::new(ManifestMeta::default(), vec![], files))
    }

    fn run(dir: &Path, m: &Manifest) -> FileCheck {
        verify(dir, m, &AtomicBool::new(false), &mut |_, _| {}).unwrap()
    }

    #[test]
    fn all_good_files_pass() {
        let (dir, m) = game();
        let c = run(dir.path(), &m);
        assert_eq!(c.bad_files, 0);
        assert_eq!(c.good.len(), 3);
        assert_eq!(c.good[0], (hex::encode(m.files[0].sha1), "Bin/game.exe".to_string()));
    }

    #[test]
    fn missing_and_wrong_size_are_bad_without_reading() {
        let (dir, m) = game();
        std::fs::remove_file(dir.path().join("Bin/game.exe")).unwrap();
        std::fs::write(dir.path().join("data/a.pak"), b"aaaa").unwrap();
        let c = run(dir.path(), &m);
        assert_eq!(c.bad_files, 2);
        assert_eq!(c.bad_bytes, (FILES[0].1.len() + FILES[1].1.len()) as u64);
        assert_eq!(c.good.len(), 1);
    }

    #[test]
    fn same_size_different_bytes_is_bad() {
        let (dir, m) = game();
        std::fs::write(dir.path().join("data/a.pak"), b"aaaaaaaabaaaaaaa").unwrap();
        let c = run(dir.path(), &m);
        assert_eq!(c.bad_files, 1);
        assert_eq!(c.bad_bytes, 16);
    }

    #[test]
    fn unsafe_names_are_bad() {
        let (dir, mut m) = game();
        m.files[2].filename = "../outside.cfg".into();
        assert_eq!(run(dir.path(), &m).bad_files, 1);
    }

    #[test]
    fn stop_returns_none() {
        let (dir, m) = game();
        let stop = AtomicBool::new(true);
        assert!(verify(dir.path(), &m, &stop, &mut |_, _| {}).is_none());
        stop.store(false, Ordering::Relaxed);
        assert!(verify(dir.path(), &m, &stop, &mut |_, _| {}).is_some());
    }

    #[test]
    fn progress_counts_every_file() {
        let (dir, m) = game();
        let mut seen = Vec::new();
        verify(dir.path(), &m, &AtomicBool::new(false), &mut |d, t| seen.push((d, t)));
        assert_eq!(seen, vec![(1, 3), (2, 3), (3, 3)]);
    }
}
