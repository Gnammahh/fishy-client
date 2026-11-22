//! Games already installed by Epic's own launcher (spec §6g). Read-only: nothing here ever
//! writes to Epic's folders.
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// One game from Epic's list, with only the fields Fishy Client uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpicItem {
    pub app_name: String,
    /// Epic's `DisplayName`; the Library's title is shown instead when there is one.
    pub title: String,
    pub install_path: PathBuf,
    pub installation_guid: String,
    pub version: String,
    pub namespace: String,
    pub catalog_item_id: String,
}

/// The `.item` file as Epic writes it. Everything else in it is ignored.
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawItem {
    #[serde(default)]
    app_name: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    install_location: String,
    #[serde(default)]
    installation_guid: String,
    #[serde(default)]
    app_version_string: String,
    #[serde(default)]
    catalog_namespace: String,
    #[serde(default)]
    catalog_item_id: String,
    #[serde(default)]
    main_game_app_name: String,
    #[serde(default, rename = "bIsIncompleteInstall")]
    incomplete: bool,
}

/// `%PROGRAMDATA%\Epic\EpicGamesLauncher\Data\Manifests`, where Epic keeps one `.item` per install.
pub fn epic_manifests_dir() -> Option<PathBuf> {
    let base = std::env::var_os("PROGRAMDATA")?;
    Some(PathBuf::from(base).join("Epic").join("EpicGamesLauncher").join("Data").join("Manifests"))
}

fn read_item(path: &Path) -> Option<EpicItem> {
    let bytes = std::fs::read(path).ok()?;
    let bytes = bytes.strip_prefix("\u{FEFF}".as_bytes()).unwrap_or(&bytes);
    let raw: RawItem = serde_json::from_slice(bytes).ok()?;
    let addon = !raw.main_game_app_name.is_empty() && raw.main_game_app_name != raw.app_name;
    let install_path = PathBuf::from(&raw.install_location);
    if raw.incomplete || addon || raw.app_name.is_empty() || raw.install_location.is_empty() || !install_path.is_dir() {
        return None;
    }
    Some(EpicItem {
        app_name: raw.app_name,
        title: raw.display_name,
        install_path,
        installation_guid: raw.installation_guid,
        version: raw.app_version_string,
        namespace: raw.catalog_namespace,
        catalog_item_id: raw.catalog_item_id,
    })
}

/// Every fully installed game in Epic's list (add-ons, unfinished installs and missing
/// folders left out), sorted by title, one per app.
pub fn scan(dir: &Path) -> Vec<EpicItem> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut items: Vec<EpicItem> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("item")) && p.is_file())
        .filter_map(|p| read_item(&p))
        .collect();
    items.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()).then_with(|| a.app_name.cmp(&b.app_name)));
    let mut seen = std::collections::HashSet::new();
    items.retain(|i| seen.insert(i.app_name.clone()));
    items
}

/// The game's own copy of its manifest, `<folder>\.egstore\<InstallationGuid>.manifest`.
pub fn local_manifest(item: &EpicItem) -> Option<Vec<u8>> {
    let guid = &item.installation_guid;
    if guid.is_empty() || guid.contains(['/', '\\', ':', '.']) {
        return None;
    }
    std::fs::read(item.install_path.join(".egstore").join(format!("{guid}.manifest"))).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item_json(app: &str, title: &str, folder: &Path, guid: &str) -> serde_json::Value {
        serde_json::json!({
            "FormatVersion": 0,
            "bIsIncompleteInstall": false,
            "AppName": app,
            "DisplayName": title,
            "InstallLocation": folder,
            "InstallationGuid": guid,
            "AppVersionString": "1.0.3",
            "CatalogNamespace": "ns",
            "CatalogItemId": "cat",
            "MainGameAppName": "",
            "LaunchExecutable": "Game.exe",
        })
    }

    fn write_item(dir: &Path, guid: &str, json: &serde_json::Value) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(format!("{guid}.item")), serde_json::to_vec(json).unwrap()).unwrap();
    }

    struct Env {
        dir: tempfile::TempDir,
    }
    impl Env {
        fn new() -> Self {
            Self { dir: tempfile::tempdir().unwrap() }
        }
        fn manifests(&self) -> PathBuf {
            self.dir.path().join("Manifests")
        }
        fn game_folder(&self, name: &str) -> PathBuf {
            let p = self.dir.path().join("Epic Games").join(name);
            std::fs::create_dir_all(&p).unwrap();
            p
        }
    }

    #[test]
    fn reads_a_game() {
        let e = Env::new();
        let folder = e.game_folder("Sol");
        write_item(&e.manifests(), "AAA", &item_json("Sol", "Sol Cesto", &folder, "AAA"));
        assert_eq!(
            scan(&e.manifests()),
            vec![EpicItem {
                app_name: "Sol".into(),
                title: "Sol Cesto".into(),
                install_path: folder,
                installation_guid: "AAA".into(),
                version: "1.0.3".into(),
                namespace: "ns".into(),
                catalog_item_id: "cat".into(),
            }]
        );
    }

    #[test]
    fn addons_skipped_main_kept() {
        let e = Env::new();
        let folder = e.game_folder("Fortnite");
        write_item(&e.manifests(), "MAIN", &item_json("Fortnite", "Fortnite", &folder, "MAIN"));
        let mut addon = item_json("aa31", "Save the World", &folder, "ADD");
        addon["MainGameAppName"] = "Fortnite".into();
        write_item(&e.manifests(), "ADD", &addon);
        let names: Vec<String> = scan(&e.manifests()).into_iter().map(|i| i.app_name).collect();
        assert_eq!(names, vec!["Fortnite".to_string()]);
    }

    #[test]
    fn main_game_naming_itself_is_kept() {
        let e = Env::new();
        let folder = e.game_folder("Sol");
        let mut j = item_json("Sol", "Sol Cesto", &folder, "AAA");
        j["MainGameAppName"] = "Sol".into();
        write_item(&e.manifests(), "AAA", &j);
        assert_eq!(scan(&e.manifests()).len(), 1);
    }

    #[test]
    fn incomplete_install_skipped() {
        let e = Env::new();
        let mut j = item_json("Sol", "Sol Cesto", &e.game_folder("Sol"), "AAA");
        j["bIsIncompleteInstall"] = true.into();
        write_item(&e.manifests(), "AAA", &j);
        assert!(scan(&e.manifests()).is_empty());
    }

    #[test]
    fn missing_folder_skipped() {
        let e = Env::new();
        let gone = e.dir.path().join("Epic Games").join("Gone");
        write_item(&e.manifests(), "AAA", &item_json("Gone", "Gone", &gone, "AAA"));
        assert!(scan(&e.manifests()).is_empty());
    }

    #[test]
    fn broken_json_and_other_files_skipped() {
        let e = Env::new();
        let folder = e.game_folder("Sol");
        write_item(&e.manifests(), "AAA", &item_json("Sol", "Sol Cesto", &folder, "AAA"));
        std::fs::write(e.manifests().join("BAD.item"), b"{ not json").unwrap();
        std::fs::write(e.manifests().join("notes.txt"), b"{}").unwrap();
        std::fs::create_dir_all(e.manifests().join("Pending")).unwrap();
        assert_eq!(scan(&e.manifests()).len(), 1);
    }

    #[test]
    fn reads_item_with_bom() {
        let e = Env::new();
        let folder = e.game_folder("rl");
        let mut bytes = "\u{FEFF}".as_bytes().to_vec();
        bytes.extend(serde_json::to_vec(&item_json("Sugar", "Rocket League®", &folder, "RL")).unwrap());
        std::fs::create_dir_all(e.manifests()).unwrap();
        std::fs::write(e.manifests().join("RL.item"), bytes).unwrap();
        assert_eq!(scan(&e.manifests())[0].title, "Rocket League®");
    }

    #[test]
    fn sorted_by_title_and_one_per_app() {
        let e = Env::new();
        let b = e.game_folder("b");
        let a = e.game_folder("a");
        write_item(&e.manifests(), "B", &item_json("B", "beta", &b, "B"));
        write_item(&e.manifests(), "A", &item_json("A", "Alpha", &a, "A"));
        write_item(&e.manifests(), "A2", &item_json("A", "Alpha", &a, "A2"));
        let titles: Vec<String> = scan(&e.manifests()).into_iter().map(|i| i.title).collect();
        assert_eq!(titles, vec!["Alpha".to_string(), "beta".to_string()]);
    }

    #[test]
    fn missing_dir_is_empty() {
        let e = Env::new();
        assert!(scan(&e.manifests()).is_empty());
    }

    #[test]
    fn local_manifest_found_and_missing() {
        let e = Env::new();
        let folder = e.game_folder("Sol");
        write_item(&e.manifests(), "AAA", &item_json("Sol", "Sol Cesto", &folder, "AAA"));
        let item = scan(&e.manifests()).remove(0);
        assert_eq!(local_manifest(&item), None);
        std::fs::create_dir_all(folder.join(".egstore")).unwrap();
        std::fs::write(folder.join(".egstore").join("AAA.manifest"), b"bytes").unwrap();
        assert_eq!(local_manifest(&item), Some(b"bytes".to_vec()));
    }
}
