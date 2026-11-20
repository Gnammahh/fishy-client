//! What Fishy Client keeps on disk for installs (spec §6c), under `%APPDATA%\FishyClient\`:
//! `installed.json`, `manifests\<app>.manifest`, and `downloads\<app>\{job.json,manifest,resume.txt}`.
//! Never tokens.
use super::manifest::Manifest;
use super::InstallError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io;
use std::path::{Component, Path, PathBuf};

/// A download that has started (running, paused or failed). Saved as `job.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobInfo {
    pub app_name: String,
    pub title: String,
    pub namespace: String,
    pub catalog_item_id: String,
    pub install_path: PathBuf,
    /// Whole-game download size (compressed).
    pub download_size: u64,
    pub install_size: u64,
    pub base_urls: Vec<String>,
}

/// One entry of `installed.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledGame {
    pub app_name: String,
    pub title: String,
    pub version: String,
    pub install_path: PathBuf,
    pub executable: String,
    pub launch_parameters: String,
    pub install_size: u64,
    pub installed_at: DateTime<Utc>,
}

/// Where a manifest file goes inside the install folder. Refuses names that would escape it
/// (`..`, absolute paths, drive letters), so a bad manifest can't write anywhere else.
pub fn file_path(root: &Path, name: &str) -> Result<PathBuf, InstallError> {
    let bad = || InstallError::BadManifest(format!("unsafe file name {name:?}"));
    let mut out = root.to_path_buf();
    for piece in name.split(['/', '\\']) {
        let mut parts = Path::new(piece).components();
        match (parts.next(), parts.next()) {
            (Some(Component::Normal(p)), None) if !piece.contains(':') => out.push(p),
            _ => return Err(bad()),
        }
    }
    if out == root {
        return Err(bad());
    }
    Ok(out)
}

/// App names become folder names, so they must be one plain path piece.
fn check_app(app: &str) -> io::Result<()> {
    let plain = !app.is_empty() && app != "." && app != ".." && !app.contains(['/', '\\', ':']);
    if plain {
        Ok(())
    } else {
        Err(io::Error::new(io::ErrorKind::InvalidInput, "bad app name"))
    }
}

fn ignore_missing(r: io::Result<()>) -> io::Result<()> {
    match r {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

/// Writes next to the target, then renames, so a crash never leaves half a JSON file.
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

pub struct InstallStore {
    root: PathBuf,
}

impl InstallStore {
    /// `%APPDATA%\FishyClient`, like the library cache.
    pub fn default_location() -> Self {
        let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
        Self::at(base.join("FishyClient"))
    }

    pub fn at(root: PathBuf) -> Self {
        Self { root }
    }

    fn job_dir(&self, app: &str) -> io::Result<PathBuf> {
        check_app(app)?;
        Ok(self.root.join("downloads").join(app))
    }

    fn saved_manifest(&self, app: &str) -> io::Result<PathBuf> {
        check_app(app)?;
        Ok(self.root.join("manifests").join(format!("{app}.manifest")))
    }

    fn installed_file(&self) -> PathBuf {
        self.root.join("installed.json")
    }

    pub fn save_job(&self, job: &JobInfo, manifest_bytes: &[u8]) -> io::Result<()> {
        let dir = self.job_dir(&job.app_name)?;
        write_atomic(&dir.join("manifest"), manifest_bytes)?;
        write_atomic(&dir.join("job.json"), &serde_json::to_vec_pretty(job).map_err(io::Error::other)?)
    }

    /// Every saved download; unreadable ones are skipped.
    pub fn pending_jobs(&self) -> Vec<JobInfo> {
        let Ok(entries) = std::fs::read_dir(self.root.join("downloads")) else { return Vec::new() };
        let mut jobs: Vec<JobInfo> = entries
            .flatten()
            .filter_map(|e| std::fs::read(e.path().join("job.json")).ok())
            .filter_map(|bytes| serde_json::from_slice::<JobInfo>(&bytes).ok())
            .filter(|j| check_app(&j.app_name).is_ok())
            .collect();
        jobs.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        jobs
    }

    pub fn job_manifest(&self, app: &str) -> io::Result<Vec<u8>> {
        std::fs::read(self.job_dir(app)?.join("manifest"))
    }

    /// Files listed in resume.txt that still exist in the install folder.
    pub fn finished_files(&self, job: &JobInfo) -> HashSet<String> {
        let Ok(dir) = self.job_dir(&job.app_name) else { return HashSet::new() };
        let text = std::fs::read_to_string(dir.join("resume.txt")).unwrap_or_default();
        text.lines()
            .filter_map(|line| line.split_once(':').map(|(_, name)| name.to_string()))
            .filter(|name| file_path(&job.install_path, name).is_ok_and(|p| p.is_file()))
            .collect()
    }

    /// Appends `<sha1 hex>:<file name>` to resume.txt and flushes it.
    pub fn mark_finished(&self, app: &str, sha1_hex: &str, filename: &str) -> io::Result<()> {
        use std::io::Write;
        let dir = self.job_dir(app)?;
        std::fs::create_dir_all(&dir)?;
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("resume.txt"))?;
        writeln!(f, "{sha1_hex}:{filename}")
    }

    pub fn remove_job(&self, app: &str) -> io::Result<()> {
        ignore_missing(std::fs::remove_dir_all(self.job_dir(app)?))
    }

    pub fn installed(&self) -> Vec<InstalledGame> {
        std::fs::read(self.installed_file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    fn save_installed(&self, games: &[InstalledGame]) -> io::Result<()> {
        write_atomic(&self.installed_file(), &serde_json::to_vec_pretty(games).map_err(io::Error::other)?)
    }

    /// Records the game, keeps its manifest for uninstall/updates, and drops the download folder.
    pub fn finish_install(&self, job: &JobInfo, m: &Manifest) -> io::Result<InstalledGame> {
        let saved = self.saved_manifest(&job.app_name)?;
        write_atomic(&saved, &self.job_manifest(&job.app_name)?)?;
        let game = InstalledGame {
            app_name: job.app_name.clone(),
            title: job.title.clone(),
            version: m.meta.build_version.clone(),
            install_path: job.install_path.clone(),
            executable: m.meta.launch_exe.clone(),
            launch_parameters: m.meta.launch_command.clone(),
            install_size: m.install_size(),
            installed_at: Utc::now(),
        };
        let mut games: Vec<InstalledGame> = self.installed().into_iter().filter(|g| g.app_name != job.app_name).collect();
        games.push(game.clone());
        games.sort_by(|a, b| a.app_name.cmp(&b.app_name));
        self.save_installed(&games)?;
        self.remove_job(&job.app_name)?;
        Ok(game)
    }

    pub fn installed_manifest(&self, app: &str) -> io::Result<Vec<u8>> {
        std::fs::read(self.saved_manifest(app)?)
    }

    /// Removes the record and the saved manifest.
    pub fn forget_installed(&self, app: &str) -> io::Result<()> {
        let path = self.saved_manifest(app)?;
        let games: Vec<InstalledGame> = self.installed().into_iter().filter(|g| g.app_name != app).collect();
        self.save_installed(&games)?;
        ignore_missing(std::fs::remove_file(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::manifest::testbuild::{build_binary, sample};

    fn job(install: &Path) -> JobInfo {
        JobInfo {
            app_name: "Tide".into(),
            title: "Tidepool".into(),
            namespace: "ns".into(),
            catalog_item_id: "cat".into(),
            install_path: install.to_path_buf(),
            download_size: 900,
            install_size: 1224,
            base_urls: vec!["https://cdn/x".into()],
        }
    }

    #[test]
    fn file_path_joins_and_refuses_escapes() {
        let root = Path::new(r"C:\Games\Tide");
        assert_eq!(file_path(root, "Binaries/Win64/Tide.exe").unwrap(), root.join("Binaries").join("Win64").join("Tide.exe"));
        for bad in ["../evil.dll", "a/../../evil", "/abs", r"C:\Windows\x", "C:x", "a//b", "", "a/./b", r"a\..\..\b"] {
            assert!(matches!(file_path(root, bad), Err(InstallError::BadManifest(_))), "{bad}");
        }
    }

    #[test]
    fn job_round_trip_and_pending_list() {
        let dir = tempfile::tempdir().unwrap();
        let store = InstallStore::at(dir.path().join("data"));
        assert!(store.pending_jobs().is_empty());
        let j = job(&dir.path().join("game"));
        store.save_job(&j, b"MANIFEST").unwrap();
        assert_eq!(store.pending_jobs(), vec![j.clone()]);
        assert_eq!(store.job_manifest("Tide").unwrap(), b"MANIFEST");
        std::fs::create_dir_all(dir.path().join("data/downloads/broken")).unwrap();
        assert_eq!(store.pending_jobs().len(), 1, "a folder without job.json is skipped");
        store.remove_job("Tide").unwrap();
        assert!(store.pending_jobs().is_empty());
        store.remove_job("Tide").unwrap();
    }

    #[test]
    fn resume_skips_only_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        let store = InstallStore::at(dir.path().join("data"));
        let game = dir.path().join("game");
        let j = job(&game);
        store.save_job(&j, b"M").unwrap();
        std::fs::create_dir_all(game.join("sub")).unwrap();
        std::fs::write(game.join("a.txt"), "a").unwrap();
        std::fs::write(game.join("sub").join("b.txt"), "b").unwrap();
        store.mark_finished("Tide", "aa", "a.txt").unwrap();
        store.mark_finished("Tide", "bb", "sub/b.txt").unwrap();
        assert_eq!(store.finished_files(&j), HashSet::from(["a.txt".to_string(), "sub/b.txt".to_string()]));
        std::fs::remove_file(game.join("sub").join("b.txt")).unwrap();
        assert_eq!(store.finished_files(&j), HashSet::from(["a.txt".to_string()]));
    }

    #[test]
    fn finish_install_moves_manifest_and_clears_job() {
        let dir = tempfile::tempdir().unwrap();
        let store = InstallStore::at(dir.path().join("data"));
        let j = job(&dir.path().join("game"));
        let bytes = build_binary(&sample(), true);
        store.save_job(&j, &bytes).unwrap();
        let game = store.finish_install(&j, &sample()).unwrap();
        assert_eq!(game.version, "1.0.0-CL1");
        assert_eq!(game.executable, "Binaries/Tidepool.exe");
        assert_eq!(game.launch_parameters, "-fish");
        assert_eq!(game.install_size, 1224);
        assert_eq!(store.installed(), vec![game]);
        assert_eq!(store.installed_manifest("Tide").unwrap(), bytes);
        assert!(store.pending_jobs().is_empty());
        assert!(!dir.path().join("data/downloads/Tide").exists());
    }

    #[test]
    fn installed_json_round_trip_and_forget() {
        let dir = tempfile::tempdir().unwrap();
        let store = InstallStore::at(dir.path().join("data"));
        let j = job(&dir.path().join("game"));
        store.save_job(&j, b"M1").unwrap();
        store.finish_install(&j, &sample()).unwrap();
        let other = JobInfo { app_name: "Other".into(), ..j.clone() };
        store.save_job(&other, b"M2").unwrap();
        store.finish_install(&other, &sample()).unwrap();
        // Re-installing replaces the record instead of adding a second one.
        store.save_job(&j, b"M3").unwrap();
        store.finish_install(&j, &sample()).unwrap();
        let apps: Vec<String> = store.installed().into_iter().map(|g| g.app_name).collect();
        assert_eq!(apps, vec!["Other", "Tide"]);
        store.forget_installed("Tide").unwrap();
        let apps: Vec<String> = store.installed().into_iter().map(|g| g.app_name).collect();
        assert_eq!(apps, vec!["Other"]);
        assert!(store.installed_manifest("Tide").is_err());
        std::fs::write(dir.path().join("data/installed.json"), "garbage").unwrap();
        assert!(store.installed().is_empty());
    }

    #[test]
    fn app_name_with_path_characters_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = InstallStore::at(dir.path().to_path_buf());
        for bad in ["..", "a/b", r"a\b", "", "C:"] {
            let j = JobInfo { app_name: bad.into(), ..job(dir.path()) };
            assert_eq!(store.save_job(&j, b"M").unwrap_err().kind(), io::ErrorKind::InvalidInput, "{bad}");
        }
    }
}
