use super::Game;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// What's saved on disk: the game list only, never tokens.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CachedLibrary {
    pub updated_at: DateTime<Utc>,
    pub games: Vec<Game>,
}

/// `%APPDATA%\FishyClient\library.json` (spec §6b). Deleted on Log out.
pub struct LibraryCache {
    path: PathBuf,
}

impl LibraryCache {
    /// The real location under the user's roaming AppData folder.
    pub fn default_location() -> Self {
        let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
        Self::at(base.join("FishyClient").join("library.json"))
    }

    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The saved library, or None if there is none or it can't be read.
    pub fn load(&self) -> Option<CachedLibrary> {
        let text = std::fs::read_to_string(&self.path).ok()?;
        serde_json::from_str(&text).ok()
    }

    pub fn save(&self, library: &CachedLibrary) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string(library).map_err(std::io::Error::other)?;
        std::fs::write(&self.path, json)
    }

    /// Deletes the saved library. Fine if it doesn't exist.
    pub fn clear(&self) -> std::io::Result<()> {
        match std::fs::remove_file(&self.path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_cache(name: &str) -> LibraryCache {
        let dir = std::env::temp_dir().join(format!("fishy-cache-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        LibraryCache::at(dir.join("library.json"))
    }

    fn sample() -> CachedLibrary {
        CachedLibrary {
            updated_at: "2026-11-20T20:00:00Z".parse().unwrap(),
            games: vec![Game {
                app_name: "A".into(),
                namespace: "n".into(),
                catalog_item_id: "a".into(),
                title: "Tidepool Odyssey".into(),
                cover_url: Some("https://cdn/a.jpg".into()),
            }],
        }
    }

    #[test]
    fn missing_cache_loads_none() {
        assert_eq!(temp_cache("missing").load(), None);
    }

    #[test]
    fn cache_round_trips_and_creates_its_folder() {
        let c = temp_cache("roundtrip");
        c.save(&sample()).unwrap();
        assert_eq!(c.load(), Some(sample()));
    }

    #[test]
    fn clear_deletes_the_file_and_is_fine_when_missing() {
        let c = temp_cache("clear");
        c.save(&sample()).unwrap();
        c.clear().unwrap();
        assert_eq!(c.load(), None);
        c.clear().unwrap();
    }

    #[test]
    fn damaged_cache_loads_none() {
        let c = temp_cache("damaged");
        c.save(&sample()).unwrap();
        std::fs::write(c.path(), "not json").unwrap();
        assert_eq!(c.load(), None);
    }
}
