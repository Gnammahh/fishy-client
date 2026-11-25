//! Games already installed by Epic's own launcher (spec §6g). Read-only: nothing here ever
//! writes to Epic's folders.
use crate::install::manifest::{parse_manifest, Manifest};
use crate::install::store::{file_path, InstallStore, InstalledGame, JobInfo};
use serde::Deserialize;
use std::io;
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
    /// For an add-on piece in its game's folder (Fortnite's Save the World Content), the game's
    /// app name (spec §6n). Such pieces are imported with their game, never on their own.
    pub parent: Option<String>,
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
    Some(crate::epic::LAUNCHER_MANIFESTS_DIR.iter().fold(PathBuf::from(base), |p, part| p.join(part)))
}

fn read_item(path: &Path) -> Option<EpicItem> {
    let bytes = std::fs::read(path).ok()?;
    let bytes = bytes.strip_prefix("\u{FEFF}".as_bytes()).unwrap_or(&bytes);
    let raw: RawItem = serde_json::from_slice(bytes).ok()?;
    let addon = !raw.main_game_app_name.is_empty() && raw.main_game_app_name != raw.app_name;
    let install_path = PathBuf::from(&raw.install_location);
    if raw.incomplete || raw.app_name.is_empty() || raw.install_location.is_empty() || !install_path.is_dir() {
        return None;
    }
    Some(EpicItem {
        parent: addon.then(|| raw.main_game_app_name.clone()),
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
    // An add-on only counts in its game's folder; elsewhere it isn't a piece Fishy can handle.
    let homes: Vec<(String, PathBuf)> = items.iter().filter(|i| i.parent.is_none()).map(|i| (i.app_name.clone(), i.install_path.clone())).collect();
    items.retain(|i| i.parent.as_ref().is_none_or(|p| homes.iter().any(|(app, path)| app == p && *path == i.install_path)));
    items
}

/// A game's add-on pieces from Epic's list (same folder), by app name (spec §6n).
pub fn pieces_of<'a>(items: &'a [EpicItem], game: &EpicItem) -> Vec<&'a EpicItem> {
    let mut pieces: Vec<&EpicItem> = items.iter().filter(|i| i.parent.as_deref() == Some(game.app_name.as_str()) && i.install_path == game.install_path).collect();
    pieces.sort_by(|a, b| a.app_name.cmp(&b.app_name));
    pieces
}

/// The game's own copy of its manifest, `<folder>\.egstore\<InstallationGuid>.manifest`.
pub fn local_manifest(item: &EpicItem) -> Option<Vec<u8>> {
    let guid = &item.installation_guid;
    if guid.is_empty() || guid.contains(['/', '\\', ':', '.']) {
        return None;
    }
    std::fs::read(item.install_path.join(crate::epic::EGSTORE_DIR).join(format!("{guid}.manifest"))).ok()
}

/// The manifest, if it parses. Its own app name isn't compared: Epic often puts the build's name
/// there ("SugarStaging" for "Sugar"). The file is tied to this install by its InstallationGuid.
pub fn usable_manifest(_item: &EpicItem, bytes: &[u8]) -> Option<Manifest> {
    parse_manifest(bytes).ok()
}

/// What the size check found.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FileCheck {
    /// `(sha1 hex, name)` of every file that's there with the right size.
    pub good: Vec<(String, String)>,
    pub bad_files: usize,
    /// The manifest sizes of the missing or damaged files.
    pub bad_bytes: u64,
}

impl FileCheck {
    pub fn all_good(&self) -> bool {
        self.bad_files == 0
    }
}

/// Every manifest file must exist with the manifest's size (no SHA-1: too slow for big games).
pub fn check_files(root: &Path, m: &Manifest) -> FileCheck {
    let mut check = FileCheck::default();
    for f in &m.files {
        let ok = file_path(root, &f.filename).ok().and_then(|p| std::fs::metadata(p).ok()).is_some_and(|md| md.is_file() && md.len() == f.size());
        if ok {
            check.good.push((hex::encode(f.sha1), f.filename.clone()));
        } else {
            check.bad_files += 1;
            check.bad_bytes += f.size();
        }
    }
    check
}

/// The download record for an imported game; `imported` keeps Cancel off its files.
pub fn import_job(item: &EpicItem, title: &str, m: &Manifest, base_urls: Vec<String>, parts: Option<Vec<String>>) -> JobInfo {
    JobInfo {
        app_name: item.app_name.clone(),
        title: title.to_string(),
        namespace: item.namespace.clone(),
        catalog_item_id: item.catalog_item_id.clone(),
        install_path: item.install_path.clone(),
        download_size: m.chunks.iter().map(|c| c.file_size).sum(),
        install_size: m.install_size(),
        base_urls,
        update_from: None,
        imported: true,
        parts,
        parent: item.parent.clone(),
    }
}

/// Every file is there: record the game as installed, like a finished install.
pub fn add_installed(store: &InstallStore, job: &JobInfo, bytes: &[u8], m: &Manifest) -> io::Result<InstalledGame> {
    store.save_job(job, bytes)?;
    store.finish_install(job, m)
}

/// Some files are missing: save a download whose resume list already holds the good ones,
/// so only the rest are downloaded.
pub fn save_repair(store: &InstallStore, job: &JobInfo, bytes: &[u8], check: &FileCheck) -> io::Result<()> {
    store.save_job(job, bytes)?;
    for (sha1, name) in &check.good {
        store.mark_finished(&job.app_name, sha1, name)?;
    }
    Ok(())
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
                parent: None,
            }]
        );
    }

    #[test]
    fn scan_keeps_add_ons_with_their_game() {
        let e = Env::new();
        let folder = e.game_folder("Fortnite");
        write_item(&e.manifests(), "MAIN", &item_json("Fortnite", "Fortnite", &folder, "MAIN"));
        let mut addon = item_json("aa31", "Fortnite Save the World Content", &folder, "ADD");
        addon["MainGameAppName"] = "Fortnite".into();
        write_item(&e.manifests(), "ADD", &addon);
        let mut elsewhere = item_json("bb42", "Somewhere else", &e.game_folder("Other"), "OTH");
        elsewhere["MainGameAppName"] = "Fortnite".into();
        write_item(&e.manifests(), "OTH", &elsewhere);
        let got: Vec<(String, Option<String>)> = scan(&e.manifests()).into_iter().map(|i| (i.app_name, i.parent)).collect();
        assert_eq!(got, vec![("Fortnite".into(), None), ("aa31".into(), Some("Fortnite".into()))]);
    }

    #[test]
    fn pieces_of_finds_the_add_ons() {
        let e = Env::new();
        let folder = e.game_folder("Fortnite");
        write_item(&e.manifests(), "MAIN", &item_json("Fortnite", "Fortnite", &folder, "MAIN"));
        for (app, guid) in [("bb", "B"), ("aa", "A")] {
            let mut addon = item_json(app, app, &folder, guid);
            addon["MainGameAppName"] = "Fortnite".into();
            write_item(&e.manifests(), guid, &addon);
        }
        write_item(&e.manifests(), "SOL", &item_json("Sol", "Sol", &e.game_folder("Sol"), "SOL"));
        let items = scan(&e.manifests());
        let game = items.iter().find(|i| i.app_name == "Fortnite").unwrap();
        let pieces: Vec<&str> = pieces_of(&items, game).into_iter().map(|i| i.app_name.as_str()).collect();
        assert_eq!(pieces, vec!["aa", "bb"]);
    }

    #[test]
    fn import_job_carries_parts_and_parent() {
        let e = Env::new();
        let mut item = epic_game(&e);
        item.parent = Some("Fortnite".into());
        let j = import_job(&item, "Piece", &sample(), Vec::new(), Some(vec!["A".into()]));
        assert_eq!(j.parts, Some(vec!["A".to_string()]));
        assert_eq!(j.parent.as_deref(), Some("Fortnite"));
        assert_eq!((j.namespace.as_str(), j.catalog_item_id.as_str()), ("ns", "cat"));
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
    // ---- checking files, adding, repairing ----
    use crate::install::manifest::testbuild::{build_binary, sample};


    /// The sample game (app "Tidepool") as Epic would have installed it: every file the right size.
    fn epic_game(e: &Env) -> EpicItem {
        let folder = e.game_folder("Tidepool");
        for f in &sample().files {
            let p = file_path(&folder, &f.filename).unwrap();
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, vec![b'x'; f.size() as usize]).unwrap();
        }
        EpicItem {
            app_name: "Tidepool".into(),
            title: "Tidepool".into(),
            install_path: folder,
            installation_guid: "AAA".into(),
            version: "1.0.0-CL1".into(),
            namespace: "ns".into(),
            catalog_item_id: "cat".into(),
            parent: None,
        }
    }

    #[test]
    fn all_files_present_is_all_good() {
        let e = Env::new();
        let item = epic_game(&e);
        let c = check_files(&item.install_path, &sample());
        assert!(c.all_good());
        assert_eq!(c.good.len(), 3);
        assert_eq!((c.bad_files, c.bad_bytes), (0, 0));
        assert_eq!(c.good[1], (hex::encode([4u8; 20]), "readme.txt".to_string()));
    }

    #[test]
    fn missing_file_is_bad() {
        let e = Env::new();
        let item = epic_game(&e);
        std::fs::remove_file(item.install_path.join("readme.txt")).unwrap();
        let c = check_files(&item.install_path, &sample());
        assert!(!c.all_good());
        assert_eq!((c.bad_files, c.bad_bytes), (1, 24));
        assert_eq!(c.good.len(), 2);
    }

    #[test]
    fn wrong_size_is_bad() {
        let e = Env::new();
        let item = epic_game(&e);
        std::fs::write(item.install_path.join("Binaries").join("Tidepool.exe"), b"short").unwrap();
        let c = check_files(&item.install_path, &sample());
        assert_eq!((c.bad_files, c.bad_bytes), (1, 1200));
    }

    #[test]
    fn unsafe_name_is_bad() {
        let e = Env::new();
        let item = epic_game(&e);
        let mut m = sample();
        m.files[1].filename = "../outside.txt".into();
        let c = check_files(&item.install_path, &m);
        assert_eq!(c.bad_files, 1);
        assert!(c.good.iter().all(|(_, n)| n != "../outside.txt"));
    }

    #[test]
    fn manifest_with_a_build_name_is_used() {
        // Real Epic manifests often name the build ("SugarStaging"), not the app ("Sugar").
        let e = Env::new();
        let mut item = epic_game(&e);
        item.app_name = "Sugar".into();
        assert!(usable_manifest(&item, &build_binary(&sample(), true)).is_some());
    }

    #[test]
    fn bad_manifest_bytes_not_used() {
        let e = Env::new();
        let item = epic_game(&e);
        assert!(usable_manifest(&item, b"\x00\x01 not a manifest").is_none());
    }

    #[test]
    fn import_job_fields() {
        let e = Env::new();
        let item = epic_game(&e);
        let j = import_job(&item, "Tidepool (Library)", &sample(), vec!["https://cdn/base".into()], None);
        assert!(j.imported);
        assert_eq!(j.update_from, None);
        assert_eq!(j.title, "Tidepool (Library)");
        assert_eq!(j.install_path, item.install_path);
        assert_eq!((j.download_size, j.install_size), (900, 1224));
        assert_eq!((j.namespace.as_str(), j.catalog_item_id.as_str()), ("ns", "cat"));
    }

    #[test]
    fn add_installed_writes_record_and_manifest() {
        let e = Env::new();
        let item = epic_game(&e);
        let store = InstallStore::at(e.dir.path().join("data"));
        let job = import_job(&item, "Tidepool", &sample(), vec![], None);
        let game = add_installed(&store, &job, &build_binary(&sample(), true), &sample()).unwrap();
        assert_eq!(game.version, "1.0.0-CL1");
        assert_eq!(game.executable, "Binaries/Tidepool.exe");
        assert_eq!(game.launch_parameters, "-fish");
        assert_eq!(game.install_path, item.install_path);
        assert_eq!(store.installed(), vec![game]);
        assert!(store.installed_manifest("Tidepool").is_ok());
        assert!(store.pending_jobs().is_empty());
    }

    #[test]
    fn save_repair_lists_only_good_files() {
        let e = Env::new();
        let item = epic_game(&e);
        std::fs::remove_file(item.install_path.join("readme.txt")).unwrap();
        let store = InstallStore::at(e.dir.path().join("data"));
        let check = check_files(&item.install_path, &sample());
        let job = import_job(&item, "Tidepool", &sample(), vec![], None);
        save_repair(&store, &job, &build_binary(&sample(), true), &check).unwrap();
        let jobs = store.pending_jobs();
        assert!(jobs[0].imported);
        let names = store.finished_names(&jobs[0]);
        let want: std::collections::HashSet<String> = ["Binaries/Tidepool.exe", "empty.cfg"].iter().map(|s| s.to_string()).collect();
        assert_eq!(names, want);
        assert!(store.installed().is_empty());
    }

}
