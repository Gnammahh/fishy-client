//! The user's settings, saved as `%APPDATA%\FishyClient\settings.json` (spec §6e). No secrets.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Queue every update a Library refresh finds.
    pub auto_update: bool,
    /// Ask before playing a game that has an update waiting.
    pub warn_outdated_play: bool,
    /// Where the Install box starts (spec §6f.1). None: `%USERPROFILE%\Games`.
    pub install_folder: Option<String>,
    /// Download speed limit in MB/s (1 MB = 1,000,000 bytes), 1 to 1000. None: no limit.
    pub speed_limit_mbps: Option<u32>,
    /// Calm animations even when Windows doesn't ask for it.
    pub reduce_motion: bool,
    /// The Library's order (spec §6f.3, §6f.4): "az", "za", "installed", "size" or "played".
    pub library_sort: String,
    /// What Fishy Client does after Play (spec §6f.4): "keep" or "minimize".
    pub when_game_starts: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_update: false,
            warn_outdated_play: true,
            install_folder: None,
            speed_limit_mbps: None,
            reduce_motion: false,
            library_sort: "az".into(),
            when_game_starts: "keep".into(),
        }
    }
}

pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    pub fn default_location() -> Self {
        let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
        Self::at(base.join("FishyClient").join("settings.json"))
    }

    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    /// The saved settings; missing or damaged means the defaults.
    pub fn load(&self) -> Settings {
        std::fs::read(&self.path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self, settings: &Settings) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(settings).map_err(std::io::Error::other)?)?;
        std::fs::rename(tmp, &self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_default_when_missing_or_damaged() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::at(dir.path().join("settings.json"));
        assert_eq!(store.load(), Settings::default());
        assert!(!Settings::default().auto_update && Settings::default().warn_outdated_play);
        std::fs::write(dir.path().join("settings.json"), b"{not json").unwrap();
        assert_eq!(store.load(), Settings::default());
        std::fs::write(dir.path().join("settings.json"), br#"{"auto_update": true}"#).unwrap();
        assert_eq!(store.load(), Settings { auto_update: true, ..Settings::default() });
    }

    #[test]
    fn old_settings_file_loads_with_new_defaults() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("settings.json"), br#"{"auto_update": true, "warn_outdated_play": false}"#).unwrap();
        let s = SettingsStore::at(dir.path().join("settings.json")).load();
        assert_eq!(s.install_folder, None);
        assert_eq!(s.speed_limit_mbps, None);
        assert!(!s.reduce_motion);
        assert_eq!(s.library_sort, "az");
        assert_eq!(s.when_game_starts, "keep");
        assert!(s.auto_update && !s.warn_outdated_play);
    }

    #[test]
    fn settings_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::at(dir.path().join("sub").join("settings.json"));
        let s = Settings {
            auto_update: true,
            warn_outdated_play: false,
            install_folder: Some(r"D:\Games".into()),
            speed_limit_mbps: Some(25),
            reduce_motion: true,
            library_sort: "size".into(),
            when_game_starts: "minimize".into(),
        };
        store.save(&s).unwrap();
        assert_eq!(store.load(), s);
    }
}
