//! "Delete all my data": empties `%APPDATA%\FishyClient` (spec §6f.1).
use std::path::{Path, PathBuf};

/// What couldn't be deleted (usually a file in use).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct WipeReport {
    pub failed: Vec<PathBuf>,
}

/// Deletes everything inside `root` (not `root` itself), deepest first. A `.log` file that
/// can't be deleted (today's log is held open) is emptied instead and doesn't count as failed.
pub fn wipe_dir(root: &Path) -> WipeReport {
    let mut report = WipeReport::default();
    wipe_inside(root, &mut report);
    report
}

/// Returns true if something stays behind in `dir` (a failure or an emptied log), so the
/// folder itself can't go and that isn't counted twice.
fn wipe_inside(dir: &Path, report: &mut WipeReport) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else { return false };
    let mut kept = false;
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            if wipe_inside(&path, report) {
                kept = true;
            } else if std::fs::remove_dir(&path).is_err() {
                report.failed.push(path);
                kept = true;
            }
        } else if std::fs::remove_file(&path).is_err() {
            kept = true;
            let is_log = path.extension().is_some_and(|e| e == "log");
            let emptied = is_log && std::fs::OpenOptions::new().write(true).open(&path).and_then(|f| f.set_len(0)).is_ok();
            if !emptied {
                report.failed.push(path);
            }
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn sample(root: &Path) -> Vec<PathBuf> {
        let files = [
            "settings.json",
            "library.json",
            r"manifests\a.manifest",
            r"downloads\x\job.json",
            r"downloads\x\big.bin",
            r"logs\fishy-client.2026-11-21.log",
        ];
        files
            .iter()
            .map(|f| {
                let p = root.join(f);
                fs::create_dir_all(p.parent().unwrap()).unwrap();
                fs::write(&p, b"data").unwrap();
                p
            })
            .collect()
    }

    fn remaining(root: &Path) -> usize {
        fs::read_dir(root).unwrap().count()
    }

    #[test]
    fn wipes_everything_inside() {
        let dir = tempfile::tempdir().unwrap();
        sample(dir.path());
        assert_eq!(wipe_dir(dir.path()), WipeReport::default());
        assert!(dir.path().exists());
        assert_eq!(remaining(dir.path()), 0);
    }

    #[test]
    fn missing_root_is_fine() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(wipe_dir(&dir.path().join("nope")), WipeReport::default());
    }

    #[cfg(windows)]
    fn hold(path: &Path, share: u32) -> fs::File {
        use std::os::windows::fs::OpenOptionsExt;
        fs::OpenOptions::new().read(true).write(true).share_mode(share).open(path).unwrap()
    }

    #[cfg(windows)]
    #[test]
    fn a_locked_file_is_reported_and_the_rest_goes() {
        let dir = tempfile::tempdir().unwrap();
        let files = sample(dir.path());
        let locked = files.iter().find(|p| p.ends_with("big.bin")).unwrap().clone();
        let handle = hold(&locked, 0);
        let report = wipe_dir(dir.path());
        drop(handle);
        assert_eq!(report.failed, vec![locked.clone()]);
        for f in files.iter().filter(|p| **p != locked) {
            assert!(!f.exists(), "{f:?} should be gone");
        }
    }

    #[cfg(windows)]
    #[test]
    fn an_open_log_file_is_emptied() {
        let dir = tempfile::tempdir().unwrap();
        let files = sample(dir.path());
        let log = files.iter().find(|p| p.extension().is_some_and(|e| e == "log")).unwrap().clone();
        // How tracing-appender holds today's log: others may read and write, not delete.
        const FILE_SHARE_READ: u32 = 1;
        const FILE_SHARE_WRITE: u32 = 2;
        let handle = hold(&log, FILE_SHARE_READ | FILE_SHARE_WRITE);
        let report = wipe_dir(dir.path());
        drop(handle);
        assert_eq!(report, WipeReport::default());
        assert_eq!(fs::metadata(&log).unwrap().len(), 0);
    }
}
