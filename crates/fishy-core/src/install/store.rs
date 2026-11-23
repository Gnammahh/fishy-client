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
    /// The installed version an update started from (spec §6e). None: a normal install.
    #[serde(default)]
    pub update_from: Option<String>,
    /// Added from Epic's launcher (spec §6g): Cancel must not delete the game's files.
    #[serde(default)]
    pub imported: bool,
    /// The optional parts to install (spec §6n). None: every file.
    #[serde(default)]
    pub parts: Option<Vec<String>>,
    /// For an add-on piece (Fortnite's Save the World Content), its game (spec §6n).
    #[serde(default)]
    pub parent: Option<String>,
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
    /// When Play last started it (spec §6f.4). Records from before then have none.
    #[serde(default)]
    pub last_played: Option<DateTime<Utc>>,
    /// Seconds played through Fishy Client (spec §6p).
    #[serde(default)]
    pub playtime_secs: u64,
    /// The installed optional parts (spec §6n). None: every file.
    #[serde(default)]
    pub parts: Option<Vec<String>>,
    /// An add-on piece's game; such records are hidden from the Library (spec §6n).
    #[serde(default)]
    pub parent: Option<String>,
    /// Epic's catalog details, needed to update a piece (add-ons aren't in the Library list).
    #[serde(default)]
    pub namespace: String,
    #[serde(default)]
    pub catalog_item_id: String,
    /// The game's choices in its Manage box (spec §6t). Old records have the defaults.
    #[serde(default)]
    pub options: GameOptions,
}

/// One game's Manage box choices (spec §6t).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GameOptions {
    /// None: follow the Auto update switch in Settings.
    pub auto_update: Option<bool>,
    /// Its downloads go in front of the waiting non-priority ones.
    pub priority: bool,
    /// Extra command-line words, used only while `launch_args_on`.
    pub launch_args: String,
    pub launch_args_on: bool,
    /// The shortcut files Fishy Client made; only these are ever deleted.
    pub shortcuts: Vec<PathBuf>,
}

pub const LAUNCH_ARGS_MAX: usize = 1000;
pub const LAUNCH_ARGS_TOO_LONG: &str = "Keep it under 1000 characters.";

/// Launch options longer than 1000 characters are refused.
pub fn check_launch_args(text: &str) -> Result<(), &'static str> {
    if text.chars().count() > LAUNCH_ARGS_MAX {
        Err(LAUNCH_ARGS_TOO_LONG)
    } else {
        Ok(())
    }
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

/// Held around every read-change-write of `installed.json`, so an install finishing while Play
/// saves a play time can't lose either change.
static INSTALLED_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn installed_lock() -> std::sync::MutexGuard<'static, ()> {
    INSTALLED_LOCK.lock().unwrap_or_else(|e| e.into_inner())
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
        jobs.sort_by_key(|j| j.title.to_lowercase());
        jobs
    }

    pub fn job_manifest(&self, app: &str) -> io::Result<Vec<u8>> {
        std::fs::read(self.job_dir(app)?.join("manifest"))
    }

    /// Files listed in resume.txt that still exist in the install folder.
    pub fn finished_files(&self, job: &JobInfo) -> HashSet<String> {
        self.finished_names(job).into_iter().filter(|name| file_path(&job.install_path, name).is_ok_and(|p| p.is_file())).collect()
    }

    /// The names in resume.txt, without checking the files are still there (cheap: for the rough
    /// "already downloaded" figure shown at startup).
    pub fn finished_names(&self, job: &JobInfo) -> HashSet<String> {
        let Ok(dir) = self.job_dir(&job.app_name) else { return HashSet::new() };
        let text = std::fs::read_to_string(dir.join("resume.txt")).unwrap_or_default();
        text.lines().filter_map(|line| line.split_once(':').map(|(_, name)| name.to_string())).collect()
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
        let _lock = installed_lock();
        // An update keeps the original install date and the last play time.
        let before = self.installed().into_iter().find(|g| g.app_name == job.app_name);
        let installed_at = before.as_ref().map(|g| g.installed_at).unwrap_or_else(Utc::now);
        let last_played = before.as_ref().and_then(|g| g.last_played);
        let playtime_secs = before.as_ref().map_or(0, |g| g.playtime_secs);
        let options = before.map(|g| g.options).unwrap_or_default();
        let game = InstalledGame {
            app_name: job.app_name.clone(),
            title: job.title.clone(),
            version: m.meta.build_version.clone(),
            install_path: job.install_path.clone(),
            executable: m.meta.launch_exe.clone(),
            launch_parameters: m.meta.launch_command.clone(),
            install_size: m.install_size(),
            installed_at,
            last_played,
            playtime_secs,
            parts: job.parts.clone(),
            parent: job.parent.clone(),
            namespace: job.namespace.clone(),
            catalog_item_id: job.catalog_item_id.clone(),
            options,
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

    /// Saves when Play started the game. Nothing happens if it isn't installed.
    pub fn mark_played(&self, app: &str, at: DateTime<Utc>) -> io::Result<()> {
        let _lock = installed_lock();
        let mut games = self.installed();
        let Some(game) = games.iter_mut().find(|g| g.app_name == app) else { return Ok(()) };
        game.last_played = Some(at);
        self.save_installed(&games)
    }

    /// Adds time played through Fishy Client (spec §6p). Nothing happens if it isn't installed.
    pub fn add_playtime(&self, app: &str, secs: u64) -> io::Result<()> {
        let _lock = installed_lock();
        let mut games = self.installed();
        let Some(game) = games.iter_mut().find(|g| g.app_name == app) else { return Ok(()) };
        game.playtime_secs = game.playtime_secs.saturating_add(secs);
        self.save_installed(&games)
    }

    /// Saves the Manage box choices (spec §6t), keeping the recorded shortcuts.
    /// None: the game isn't installed.
    pub fn set_options(&self, app: &str, auto_update: Option<bool>, priority: bool, launch_args: &str, launch_args_on: bool) -> io::Result<Option<InstalledGame>> {
        let _lock = installed_lock();
        let mut games = self.installed();
        let Some(game) = games.iter_mut().find(|g| g.app_name == app) else { return Ok(None) };
        game.options.auto_update = auto_update;
        game.options.priority = priority;
        game.options.launch_args = launch_args.to_string();
        game.options.launch_args_on = launch_args_on;
        let saved = game.clone();
        self.save_installed(&games)?;
        Ok(Some(saved))
    }

    /// Records the shortcut files made for the game. Nothing happens if it isn't installed.
    pub fn set_shortcuts(&self, app: &str, paths: Vec<PathBuf>) -> io::Result<()> {
        let _lock = installed_lock();
        let mut games = self.installed();
        let Some(game) = games.iter_mut().find(|g| g.app_name == app) else { return Ok(()) };
        game.options.shortcuts = paths;
        self.save_installed(&games)
    }

    /// Removes the record and the saved manifest.
    pub fn forget_installed(&self, app: &str) -> io::Result<()> {
        let path = self.saved_manifest(app)?;
        let _lock = installed_lock();
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
            update_from: None,
            imported: false,
            parts: None,
            parent: None,
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
        assert_eq!(store.finished_names(&j).len(), 2, "names only, no file checks");
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
    fn old_job_json_without_imported_reads_as_false() {
        let j: JobInfo = serde_json::from_value(serde_json::json!({
            "app_name": "Tide", "title": "T", "namespace": "ns", "catalog_item_id": "c",
            "install_path": "C:/Games/T", "download_size": 1, "install_size": 1, "base_urls": []
        }))
        .unwrap();
        assert!(!j.imported);
    }

    #[test]
    fn old_job_json_without_update_from_loads() {
        let j: JobInfo = serde_json::from_value(serde_json::json!({
            "app_name": "Tide", "title": "T", "namespace": "ns", "catalog_item_id": "c",
            "install_path": "C:/Games/T", "download_size": 1, "install_size": 1, "base_urls": []
        }))
        .unwrap();
        assert_eq!(j.update_from, None);
    }

    #[test]
    fn finish_install_keeps_the_install_date_on_update() {
        let dir = tempfile::tempdir().unwrap();
        let store = InstallStore::at(dir.path().join("data"));
        let j = job(&dir.path().join("game"));
        store.save_job(&j, &build_binary(&sample(), true)).unwrap();
        let first = store.finish_install(&j, &sample()).unwrap();
        let update = JobInfo { update_from: Some("1.0.0-CL1".into()), ..j.clone() };
        store.save_job(&update, &build_binary(&sample(), true)).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let second = store.finish_install(&update, &sample()).unwrap();
        assert_eq!(second.installed_at, first.installed_at);
        assert_eq!(store.installed().len(), 1);
    }

    fn installed_tide(dir: &Path) -> (InstallStore, JobInfo) {
        let store = InstallStore::at(dir.join("data"));
        let j = job(&dir.join("game"));
        store.save_job(&j, &build_binary(&sample(), true)).unwrap();
        store.finish_install(&j, &sample()).unwrap();
        (store, j)
    }

    #[test]
    fn options_default_in_old_records() {
        let g: InstalledGame = serde_json::from_value(serde_json::json!({
            "app_name": "Tide", "title": "T", "version": "1", "install_path": "C:/Games/T",
            "executable": "t.exe", "launch_parameters": "", "install_size": 1,
            "installed_at": "2026-11-20T10:00:00Z"
        }))
        .unwrap();
        assert_eq!(g.options, GameOptions::default());
        assert_eq!(g.options.auto_update, None);
    }

    #[test]
    fn finish_install_keeps_options() {
        let dir = tempfile::tempdir().unwrap();
        let (store, j) = installed_tide(dir.path());
        store.set_options("Tide", Some(false), true, "-windowed", true).unwrap();
        let link = dir.path().join("Tide.lnk");
        store.set_shortcuts("Tide", vec![link.clone()]).unwrap();
        let update = JobInfo { update_from: Some("1.0.0-CL1".into()), ..j };
        store.save_job(&update, &build_binary(&sample(), true)).unwrap();
        let after = store.finish_install(&update, &sample()).unwrap();
        assert_eq!(
            after.options,
            GameOptions { auto_update: Some(false), priority: true, launch_args: "-windowed".into(), launch_args_on: true, shortcuts: vec![link] }
        );
    }

    #[test]
    fn set_options_keeps_shortcuts_and_ignores_missing_games() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = installed_tide(dir.path());
        let link = dir.path().join("Tide.lnk");
        store.set_shortcuts("Tide", vec![link.clone()]).unwrap();
        let g = store.set_options("Tide", Some(true), false, "", false).unwrap().unwrap();
        assert_eq!(g.options.shortcuts, vec![link]);
        assert_eq!(store.installed()[0].options.auto_update, Some(true));
        assert!(store.set_options("nope", None, false, "", false).unwrap().is_none());
        store.set_shortcuts("nope", vec![]).unwrap();
    }

    #[test]
    fn launch_args_limit() {
        assert!(check_launch_args(&"a".repeat(1000)).is_ok());
        assert_eq!(check_launch_args(&"a".repeat(1001)), Err(LAUNCH_ARGS_TOO_LONG));
        assert!(check_launch_args(&"é".repeat(1000)).is_ok(), "characters, not bytes");
        assert_eq!(LAUNCH_ARGS_TOO_LONG, "Keep it under 1000 characters.");
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
    #[test]
    fn play_time_is_saved_and_kept_by_an_update() {
        let dir = tempfile::tempdir().unwrap();
        let store = InstallStore::at(dir.path().join("data"));
        let j = job(&dir.path().join("game"));
        store.save_job(&j, &build_binary(&sample(), true)).unwrap();
        let first = store.finish_install(&j, &sample()).unwrap();
        assert_eq!(first.last_played, None);
        let t = "2026-11-22T10:00:00Z".parse::<DateTime<Utc>>().unwrap();
        store.mark_played("Tide", t).unwrap();
        assert_eq!(store.installed()[0].last_played, Some(t));
        store.save_job(&j, &build_binary(&sample(), true)).unwrap();
        let again = store.finish_install(&j, &sample()).unwrap();
        assert_eq!(again.last_played, Some(t), "an update keeps the play time");
        assert_eq!(again.installed_at, first.installed_at);
        let before = store.installed();
        store.mark_played("Nope", t).unwrap();
        assert_eq!(store.installed(), before);
    }

    #[test]
    fn playtime_adds_up_and_survives_an_update() {
        let dir = tempfile::tempdir().unwrap();
        let store = InstallStore::at(dir.path().join("data"));
        let j = job(&dir.path().join("Games").join("Tide"));
        store.save_job(&j, &build_binary(&sample(), true)).unwrap();
        assert_eq!(store.finish_install(&j, &sample()).unwrap().playtime_secs, 0);
        store.add_playtime("Tide", 90).unwrap();
        store.add_playtime("Tide", 30).unwrap();
        store.add_playtime("Nope", 10).unwrap();
        assert_eq!(store.installed()[0].playtime_secs, 120);
        store.save_job(&j, &build_binary(&sample(), true)).unwrap();
        assert_eq!(store.finish_install(&j, &sample()).unwrap().playtime_secs, 120, "an update keeps it");
    }

    #[test]
    fn records_from_1_3_0_read_without_parts() {
        let j: JobInfo = serde_json::from_value(serde_json::json!({
            "app_name": "Tide", "title": "T", "namespace": "ns", "catalog_item_id": "c",
            "install_path": "C:/Games/T", "download_size": 1, "install_size": 1, "base_urls": []
        }))
        .unwrap();
        assert_eq!((j.parts, j.parent), (None, None));
        let g: InstalledGame = serde_json::from_value(serde_json::json!({
            "app_name": "Tide", "title": "T", "version": "1", "install_path": "C:/G/T", "executable": "t.exe",
            "launch_parameters": "", "install_size": 1, "installed_at": "2026-11-01T00:00:00Z"
        }))
        .unwrap();
        assert_eq!((g.parts, g.parent, g.namespace.as_str(), g.catalog_item_id.as_str()), (None, None, "", ""));
        assert_eq!(g.playtime_secs, 0);
    }

    #[test]
    fn finish_install_keeps_parts_and_parent() {
        let dir = tempfile::tempdir().unwrap();
        let store = InstallStore::at(dir.path().join("data"));
        let mut j = job(&dir.path().join("Games").join("Tide"));
        j.parts = Some(vec!["A".into()]);
        j.parent = Some("Fortnite".into());
        store.save_job(&j, &build_binary(&sample(), true)).unwrap();
        let g = store.finish_install(&j, &sample()).unwrap();
        assert_eq!(g.parts, Some(vec!["A".to_string()]));
        assert_eq!(g.parent.as_deref(), Some("Fortnite"));
        assert_eq!((g.namespace.as_str(), g.catalog_item_id.as_str()), ("ns", "cat"));
        assert_eq!(store.installed()[0], g);
    }

    #[test]
    fn old_installed_json_without_play_time_loads() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        std::fs::write(
            dir.path().join("data/installed.json"),
            br#"[{"app_name":"Tide","title":"T","version":"1","install_path":"C:/G/T","executable":"t.exe","launch_parameters":"","install_size":1,"installed_at":"2026-11-01T00:00:00Z"}]"#,
        )
        .unwrap();
        let games = InstallStore::at(dir.path().join("data")).installed();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].last_played, None);
    }

    #[test]
    fn install_and_play_time_writes_both_survive() {
        // Play saving a time while another game finishes installing, at the same moment, many times.
        let dir = tempfile::tempdir().unwrap();
        let store = std::sync::Arc::new(InstallStore::at(dir.path().join("data")));
        let tide = job(&dir.path().join("game"));
        store.save_job(&tide, &build_binary(&sample(), true)).unwrap();
        store.finish_install(&tide, &sample()).unwrap();
        let t = "2026-11-22T10:00:00Z".parse::<DateTime<Utc>>().unwrap();
        for round in 0..20 {
            let reef = JobInfo { app_name: format!("Reef{round}"), title: "Reef".into(), ..job(&dir.path().join(format!("reef{round}"))) };
            store.save_job(&reef, &build_binary(&sample(), true)).unwrap();
            let s2 = store.clone();
            let player = std::thread::spawn(move || s2.mark_played("Tide", t + chrono::Duration::seconds(round)).unwrap());
            store.finish_install(&reef, &sample()).unwrap();
            player.join().unwrap();
            let games = store.installed();
            assert_eq!(games.len(), round as usize + 2, "round {round}: an install was lost");
            assert_eq!(games.iter().find(|g| g.app_name == "Tide").unwrap().last_played, Some(t + chrono::Duration::seconds(round)), "round {round}: a play time was lost");
        }
    }

}
