//! The user's settings, saved as `%APPDATA%\FishyClient\settings.json` (spec §6e). No secrets.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Queue every update a Library refresh finds.
    pub auto_update: bool,
    /// Ask before playing a game that has an update waiting.
    pub warn_outdated_play: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { auto_update: false, warn_outdated_play: true }
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
        assert_eq!(store.load(), Settings { auto_update: false, warn_outdated_play: true });
        std::fs::write(dir.path().join("settings.json"), b"{not json").unwrap();
        assert_eq!(store.load(), Settings::default());
        std::fs::write(dir.path().join("settings.json"), br#"{"auto_update": true}"#).unwrap();
        assert_eq!(store.load(), Settings { auto_update: true, warn_outdated_play: true });
    }

    #[test]
    fn settings_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::at(dir.path().join("sub").join("settings.json"));
        let s = Settings { auto_update: true, warn_outdated_play: false };
        store.save(&s).unwrap();
        assert_eq!(store.load(), s);
    }
}
