//! Showing Fishy Client's games in Epic's launcher (spec §6h): one `.item` in Epic's list plus
//! `.egstore\<GUID>.manifest` and `.mancpn` in the game folder, as Legendary's `egl_export` writes.
use crate::install::store::{write_atomic, InstalledGame};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

/// What the saved Library knows about the game, needed in Epic's entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncInfo {
    pub namespace: String,
    pub catalog_item_id: String,
    pub can_run_offline: bool,
    pub requires_ot: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Synced {
    /// The entry with this GUID is now up to date.
    Written { guid: String },
    /// Epic has its own copy of the game in another folder: left alone.
    OtherCopy,
    /// Epic's launcher isn't installed: nothing written.
    NoEpic,
}

/// 32 uppercase hex characters, like Legendary's `uuid4().hex.upper()`.
pub fn new_guid() -> String {
    uuid::Uuid::new_v4().simple().to_string().to_uppercase()
}

/// Folders compared the way Windows does: case, `/` vs `\` and a trailing slash don't matter.
fn same_folder(a: &str, b: &Path) -> bool {
    let norm = |s: &str| s.replace('/', "\\").trim_end_matches('\\').to_lowercase();
    norm(a) == norm(&b.display().to_string())
}

/// An `.item` file and its JSON.
type Entry = (PathBuf, Map<String, Value>);

/// Every readable `.item` in Epic's folder with this AppName.
fn entries_for(epic_dir: &Path, app: &str) -> Vec<Entry> {
    let Ok(dir) = std::fs::read_dir(epic_dir) else { return Vec::new() };
    dir.flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("item")) && p.is_file())
        .filter_map(|p| {
            let bytes = std::fs::read(&p).ok()?;
            let bytes = bytes.strip_prefix("\u{FEFF}".as_bytes()).unwrap_or(&bytes);
            match serde_json::from_slice::<Value>(bytes).ok()? {
                Value::Object(m) if m.get("AppName").and_then(Value::as_str) == Some(app) => Some((p, m)),
                _ => None,
            }
        })
        .collect()
}

/// This game's entry: same AppName and same folder. `Err(())` when Epic only has another copy.
fn find_ours(epic_dir: &Path, game: &InstalledGame) -> Result<Option<Entry>, ()> {
    let all = entries_for(epic_dir, &game.app_name);
    let any = !all.is_empty();
    match all.into_iter().find(|(_, m)| m.get("InstallLocation").and_then(Value::as_str).is_some_and(|l| same_folder(l, &game.install_path))) {
        Some(found) => Ok(Some(found)),
        None if any => Err(()),
        None => Ok(None),
    }
}

/// A GUID becomes a file name, so it must be plain letters and digits.
fn plain_guid(guid: &str) -> io::Result<&str> {
    if !guid.is_empty() && guid.chars().all(|c| c.is_ascii_alphanumeric()) {
        Ok(guid)
    } else {
        Err(io::Error::new(io::ErrorKind::InvalidData, "bad installation guid"))
    }
}

/// Epic's style: 4-space indent, keys sorted.
fn pretty(v: &impl Serialize) -> io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, serde_json::ser::PrettyFormatter::with_indent(b"    "));
    v.serialize(&mut ser).map_err(io::Error::other)?;
    Ok(buf)
}

/// The fields a new entry starts with (Legendary's `EGLManifest` defaults).
fn template() -> Map<String, Value> {
    let v = json!({
        "AppCategories": ["public", "games", "applications"],
        "BaseURLs": [], "ChunkDbs": [], "CompatibleApps": [], "InstallComponents": [], "InstallTags": [], "PrereqIds": [], "ProcessNames": [],
        "BuildLabel": "Live", "FormatVersion": 0,
        "bIsApplication": true, "bIsExecutable": true, "bRequiresAuth": true, "bIsManaged": false,
        "MandatoryAppFolderName": "", "FullAppName": "", "HostInstallationGuid": "", "InstallSessionId": "",
        "MainWindowProcessName": "", "TechnicalType": "", "VaultThumbnailUrl": "", "VaultTitleText": "",
    });
    match v {
        Value::Object(m) => m,
        _ => unreachable!(),
    }
}

/// Adds or updates the game's entry in Epic's list (spec §6h). `guid` is only called for a new
/// entry. Nothing is created when `epic_dir` doesn't exist.
pub fn write_entry(epic_dir: &Path, game: &InstalledGame, info: &SyncInfo, manifest: &[u8], guid: impl FnOnce() -> String) -> io::Result<Synced> {
    if !epic_dir.is_dir() {
        return Ok(Synced::NoEpic);
    }
    let (path, mut entry, guid) = match find_ours(epic_dir, game) {
        Err(()) => return Ok(Synced::OtherCopy),
        Ok(Some((path, m))) => {
            let g = m.get("InstallationGuid").and_then(Value::as_str).map(str::to_string).unwrap_or_default();
            plain_guid(&g)?;
            (path, m, g)
        }
        Ok(None) => {
            let g = guid();
            plain_guid(&g)?;
            (epic_dir.join(format!("{g}.item")), template(), g)
        }
    };
    let folder = game.install_path.display().to_string();
    let set = [
        ("AppName", json!(game.app_name)),
        ("DisplayName", json!(game.title)),
        ("AppVersionString", json!(game.version)),
        ("InstallLocation", json!(folder)),
        ("InstallSize", json!(game.install_size)),
        ("LaunchCommand", json!(game.launch_parameters)),
        ("LaunchExecutable", json!(game.executable)),
        ("InstallationGuid", json!(guid)),
        ("CatalogNamespace", json!(info.namespace)),
        ("CatalogItemId", json!(info.catalog_item_id)),
        ("bCanRunOffline", json!(info.can_run_offline)),
        ("OwnershipToken", json!(if info.requires_ot { "true" } else { "false" })),
        ("MainGameAppName", json!(game.app_name)),
        ("ManifestLocation", json!(format!("{folder}/.egstore"))),
        ("StagingLocation", json!(format!("{folder}/.egstore/bps"))),
        ("bIsIncompleteInstall", json!(false)),
        ("bNeedsValidation", json!(false)),
    ];
    for (k, v) in set {
        entry.insert(k.to_string(), v);
    }
    // The game folder's files first, so Epic never sees an entry without its file list.
    let egstore = game.install_path.join(".egstore");
    write_atomic(&egstore.join(format!("{guid}.manifest")), manifest)?;
    let mancpn: BTreeMap<&str, Value> = [
        ("AppName", json!(game.app_name)),
        ("CatalogItemId", json!(info.catalog_item_id)),
        ("CatalogNamespace", json!(info.namespace)),
        ("FormatVersion", json!(0)),
    ]
    .into_iter()
    .collect();
    write_atomic(&egstore.join(format!("{guid}.mancpn")), &pretty(&mancpn)?)?;
    let sorted: BTreeMap<String, Value> = entry.into_iter().collect();
    write_atomic(&path, &pretty(&sorted)?)?;
    Ok(Synced::Written { guid })
}

fn remove_if_there(p: &Path) -> io::Result<()> {
    match std::fs::remove_file(p) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Removes this game's entry (same folder only) and its `.egstore` files, then `.egstore` and
/// the game folder if they're empty. True when an entry was removed.
pub fn remove_entry(epic_dir: &Path, game: &InstalledGame) -> io::Result<bool> {
    if !epic_dir.is_dir() {
        return Ok(false);
    }
    let Ok(Some((path, m))) = find_ours(epic_dir, game) else { return Ok(false) };
    let guid = m.get("InstallationGuid").and_then(Value::as_str).unwrap_or_default().to_string();
    remove_if_there(&path)?;
    if plain_guid(&guid).is_ok() {
        let egstore = game.install_path.join(".egstore");
        remove_if_there(&egstore.join(format!("{guid}.manifest")))?;
        remove_if_there(&egstore.join(format!("{guid}.mancpn")))?;
        // Only empty folders go; anything Epic or the user left keeps them.
        let _ = std::fs::remove_dir(&egstore);
        let _ = std::fs::remove_dir(&game.install_path);
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use std::path::PathBuf;

    struct Env {
        dir: tempfile::TempDir,
    }
    impl Env {
        fn new() -> Self {
            let e = Self { dir: tempfile::tempdir().unwrap() };
            std::fs::create_dir_all(e.epic()).unwrap();
            std::fs::create_dir_all(e.folder()).unwrap();
            e
        }
        fn epic(&self) -> PathBuf {
            self.dir.path().join("Manifests")
        }
        fn folder(&self) -> PathBuf {
            self.dir.path().join("Games").join("Bendy")
        }
        fn record(&self) -> InstalledGame {
            InstalledGame {
                app_name: "Bendy".into(),
                title: "Bendy and the Ink Machine".into(),
                version: "2.0".into(),
                install_path: self.folder(),
                executable: "Bendy.exe".into(),
                launch_parameters: "-fish".into(),
                install_size: 1234,
                installed_at: chrono::Utc::now(),
                last_played: None,
            }
        }
        fn items(&self) -> Vec<PathBuf> {
            let mut v: Vec<PathBuf> =
                std::fs::read_dir(self.epic()).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "item")).collect();
            v.sort();
            v
        }
        fn item(&self, guid: &str) -> Value {
            serde_json::from_slice(&std::fs::read(self.epic().join(format!("{guid}.item"))).unwrap()).unwrap()
        }
        fn put_item(&self, guid: &str, v: &Value) {
            std::fs::write(self.epic().join(format!("{guid}.item")), serde_json::to_vec(v).unwrap()).unwrap();
        }
    }

    fn info() -> SyncInfo {
        SyncInfo { namespace: "ns".into(), catalog_item_id: "cat".into(), can_run_offline: true, requires_ot: false }
    }

    fn guid() -> String {
        "ABCDEF0123456789ABCDEF0123456789".into()
    }

    #[test]
    fn writes_a_new_entry() {
        let e = Env::new();
        assert_eq!(write_entry(&e.epic(), &e.record(), &info(), b"MANIFEST", guid).unwrap(), Synced::Written { guid: guid() });
        let text = std::fs::read_to_string(e.epic().join(format!("{}.item", guid()))).unwrap();
        assert!(text.starts_with("{\n    \""), "4-space indent, no BOM");
        let v = e.item(&guid());
        assert_eq!(v["AppName"], "Bendy");
        assert_eq!(v["DisplayName"], "Bendy and the Ink Machine");
        assert_eq!(v["AppVersionString"], "2.0");
        assert_eq!(v["InstallLocation"], e.folder().display().to_string());
        assert_eq!(v["InstallSize"], 1234);
        assert_eq!(v["LaunchExecutable"], "Bendy.exe");
        assert_eq!(v["LaunchCommand"], "-fish");
        assert_eq!(v["InstallationGuid"], guid());
        assert_eq!(v["CatalogNamespace"], "ns");
        assert_eq!(v["CatalogItemId"], "cat");
        assert_eq!(v["MainGameAppName"], "Bendy");
        assert_eq!(v["OwnershipToken"], "false");
        assert_eq!(v["bCanRunOffline"], true);
        assert_eq!(v["bIsIncompleteInstall"], false);
        assert_eq!(v["bNeedsValidation"], false);
        assert_eq!(v["bIsApplication"], true);
        assert_eq!(v["bIsManaged"], false);
        assert_eq!(v["BuildLabel"], "Live");
        assert_eq!(v["FormatVersion"], 0);
        assert_eq!(v["AppCategories"], json!(["public", "games", "applications"]));
        assert_eq!(v["InstallTags"], json!([]));
        assert_eq!(v["VaultTitleText"], "");
        assert_eq!(v["ManifestLocation"], format!("{}/.egstore", e.folder().display()));
        assert_eq!(v["StagingLocation"], format!("{}/.egstore/bps", e.folder().display()));
        let eg = e.folder().join(".egstore");
        assert_eq!(std::fs::read(eg.join(format!("{}.manifest", guid()))).unwrap(), b"MANIFEST");
        let cpn: Value = serde_json::from_slice(&std::fs::read(eg.join(format!("{}.mancpn", guid()))).unwrap()).unwrap();
        assert_eq!(cpn, json!({"AppName": "Bendy", "CatalogItemId": "cat", "CatalogNamespace": "ns", "FormatVersion": 0}));
    }

    #[test]
    fn new_guid_is_32_uppercase_hex() {
        let g = new_guid();
        assert_eq!(g.len(), 32);
        assert!(g.chars().all(|c| c.is_ascii_digit() || ('A'..='F').contains(&c)), "{g}");
        assert_ne!(g, new_guid());
    }

    #[test]
    fn updates_only_the_same_folder_entry() {
        let e = Env::new();
        e.put_item("E1", &json!({"AppName": "Bendy", "InstallLocation": e.folder(), "InstallationGuid": "E1", "AppVersionString": "old"}));
        let other = json!({"AppName": "Bendy", "InstallLocation": "D:\\Other\\Bendy", "InstallationGuid": "E2", "AppVersionString": "old"});
        e.put_item("E2", &other);
        let before = std::fs::read(e.epic().join("E2.item")).unwrap();
        assert_eq!(write_entry(&e.epic(), &e.record(), &info(), b"M", guid).unwrap(), Synced::Written { guid: "E1".into() });
        assert_eq!(e.item("E1")["AppVersionString"], "2.0");
        assert_eq!(std::fs::read(e.epic().join("E2.item")).unwrap(), before);
        assert_eq!(e.items().len(), 2);
        assert!(e.folder().join(".egstore").join("E1.manifest").is_file());
    }

    #[test]
    fn same_folder_ignores_case_slashes_and_trailing_slash() {
        let e = Env::new();
        let odd = format!("{}/", e.folder().display().to_string().replace('\\', "/").to_uppercase());
        e.put_item("E1", &json!({"AppName": "Bendy", "InstallLocation": odd, "InstallationGuid": "E1"}));
        assert_eq!(write_entry(&e.epic(), &e.record(), &info(), b"M", guid).unwrap(), Synced::Written { guid: "E1".into() });
        assert_eq!(e.items().len(), 1);
    }

    #[test]
    fn in_place_update_keeps_epics_other_fields() {
        let e = Env::new();
        e.put_item(
            "E1",
            &json!({"AppName": "Bendy", "InstallLocation": e.folder(), "InstallationGuid": "E1", "InstallComponents": ["X"], "InstallSessionId": "S"}),
        );
        write_entry(&e.epic(), &e.record(), &info(), b"M", guid).unwrap();
        let v = e.item("E1");
        assert_eq!(v["InstallComponents"], json!(["X"]));
        assert_eq!(v["InstallSessionId"], "S");
        assert_eq!(v["AppVersionString"], "2.0");
    }

    #[test]
    fn another_folder_only_is_left_alone() {
        let e = Env::new();
        e.put_item("E2", &json!({"AppName": "Bendy", "InstallLocation": "D:\\Other\\Bendy", "InstallationGuid": "E2"}));
        assert_eq!(write_entry(&e.epic(), &e.record(), &info(), b"M", guid).unwrap(), Synced::OtherCopy);
        assert_eq!(e.items().len(), 1);
        assert!(!e.folder().join(".egstore").exists());
    }

    #[test]
    fn no_epic_folder_writes_nothing() {
        let e = Env::new();
        let missing = e.dir.path().join("NoEpic");
        assert_eq!(write_entry(&missing, &e.record(), &info(), b"M", guid).unwrap(), Synced::NoEpic);
        assert!(!remove_entry(&missing, &e.record()).unwrap());
        assert!(!missing.exists());
        assert!(!e.folder().join(".egstore").exists());
    }

    #[test]
    fn remove_deletes_only_its_files() {
        let e = Env::new();
        write_entry(&e.epic(), &e.record(), &info(), b"M", guid).unwrap();
        let eg = e.folder().join(".egstore");
        std::fs::write(eg.join("OTHER.manifest"), b"epic").unwrap();
        assert!(remove_entry(&e.epic(), &e.record()).unwrap());
        assert!(e.items().is_empty());
        assert!(!eg.join(format!("{}.manifest", guid())).exists());
        assert!(!eg.join(format!("{}.mancpn", guid())).exists());
        assert!(eg.join("OTHER.manifest").is_file());
    }

    #[test]
    fn remove_cleans_empty_folders() {
        let e = Env::new();
        write_entry(&e.epic(), &e.record(), &info(), b"M", guid).unwrap();
        assert!(remove_entry(&e.epic(), &e.record()).unwrap());
        assert!(!e.folder().exists());
        assert!(e.folder().parent().unwrap().exists());
    }

    #[test]
    fn remove_leaves_another_folders_entry() {
        let e = Env::new();
        e.put_item("E2", &json!({"AppName": "Bendy", "InstallLocation": "D:\\Other\\Bendy", "InstallationGuid": "E2"}));
        assert!(!remove_entry(&e.epic(), &e.record()).unwrap());
        assert_eq!(e.items().len(), 1);
    }

    #[test]
    fn writing_twice_updates_the_same_entry() {
        let e = Env::new();
        write_entry(&e.epic(), &e.record(), &info(), b"M", guid).unwrap();
        let mut newer = e.record();
        newer.version = "3.0".into();
        let unused = || "SHOULDNOTBEUSED".to_string();
        assert_eq!(write_entry(&e.epic(), &newer, &info(), b"M2", unused).unwrap(), Synced::Written { guid: guid() });
        assert_eq!(e.items().len(), 1);
        assert_eq!(e.item(&guid())["AppVersionString"], "3.0");
    }
}
