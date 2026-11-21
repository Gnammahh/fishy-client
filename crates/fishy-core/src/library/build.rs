use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// One entry from Epic's library service.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct LibraryItem {
    pub namespace: String,
    #[serde(rename = "catalogItemId")]
    pub catalog_item_id: String,
    #[serde(rename = "appName", default)]
    pub app_name: Option<String>,
    #[serde(rename = "sandboxType", default)]
    pub sandbox_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct KeyImage {
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Category {
    pub path: String,
}

/// Game info from Epic's catalog service.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CatalogItem {
    pub title: String,
    #[serde(rename = "keyImages", default)]
    pub key_images: Vec<KeyImage>,
    #[serde(default)]
    pub categories: Vec<Category>,
    /// Present on DLC: points at the game it belongs to.
    #[serde(rename = "mainGameItem", default)]
    pub main_game_item: Option<serde_json::Value>,
    /// Holds `FolderName`, the install folder Epic suggests.
    #[serde(rename = "customAttributes", default)]
    pub custom_attributes: Option<serde_json::Value>,
}

impl CatalogItem {
    /// Epic's suggested install folder name (`customAttributes.FolderName.value`), if any.
    pub fn folder_name(&self) -> Option<String> {
        let value = self.custom_attributes.as_ref()?.pointer("/FolderName/value")?.as_str()?.trim();
        (!value.is_empty()).then(|| value.to_string())
    }

    /// The catalog attributes Play needs (spec §6d), with Legendary's defaults.
    pub fn launch_info(&self) -> LaunchInfo {
        let text = |key: &str| {
            let v = self.custom_attributes.as_ref()?.get(key)?.get("value")?.as_str()?.trim();
            (!v.is_empty()).then(|| v.to_string())
        };
        LaunchInfo {
            additional_command_line: text("AdditionalCommandLine"),
            requires_ot: text("OwnershipToken").is_some_and(|v| v.eq_ignore_ascii_case("true")),
            can_run_offline: !text("CanRunOffline").is_some_and(|v| v.eq_ignore_ascii_case("false")),
            third_party_store: text("ThirdPartyManagedApp").or_else(|| text("ThirdPartyManagedProvider")),
        }
    }
}

/// What Play needs from the catalog. Missing in old caches: then the defaults apply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LaunchInfo {
    /// Extra arguments (`customAttributes.AdditionalCommandLine`).
    pub additional_command_line: Option<String>,
    /// The game wants an ownership token file (`OwnershipToken`).
    pub requires_ot: bool,
    /// The game may start without Epic (`CanRunOffline`, default true).
    pub can_run_offline: bool,
    /// Another store's launcher starts it (EA app, Ubisoft Connect).
    pub third_party_store: Option<String>,
}

impl Default for LaunchInfo {
    fn default() -> Self {
        Self { additional_command_line: None, requires_ot: false, can_run_offline: true, third_party_store: None }
    }
}

/// A game shown in the Library (also what the cache stores).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Game {
    pub app_name: String,
    pub namespace: String,
    pub catalog_item_id: String,
    pub title: String,
    pub cover_url: Option<String>,
    /// False when Epic has no Windows download for it. Caches saved before this field count as true.
    #[serde(default = "yes")]
    pub installable: bool,
    #[serde(default)]
    pub launch: LaunchInfo,
}

fn yes() -> bool {
    true
}

/// Cheap checks before asking the catalog (same rules as Legendary, spec §6b).
pub fn worth_fetching(item: &LibraryItem) -> bool {
    let named = matches!(item.app_name.as_deref(), Some(name) if !name.is_empty() && name != "1");
    named && item.namespace != "ue" && item.sandbox_type.as_deref() != Some("PRIVATE")
}

/// Tall box art first, then other portrait images, then a thumbnail.
pub fn pick_cover(images: &[KeyImage]) -> Option<String> {
    ["DieselGameBoxTall", "OfferImageTall", "Thumbnail"]
        .iter()
        .find_map(|kind| images.iter().find(|i| i.kind == *kind))
        .map(|i| i.url.clone())
}

/// Turns library items + catalog info into the games to show, sorted A-Z.
/// DLC and mods are dropped; items without catalog info are kept under their app name.
/// `windows_apps` is Epic's list of app names with a Windows build (None if it couldn't be fetched:
/// then every game counts as installable). A non-Windows entry is hidden when the same namespace
/// has an installable entry with the same title (e.g. Bendy is in some libraries twice).
pub fn build_library(items: &[LibraryItem], catalog: &HashMap<String, CatalogItem>, windows_apps: Option<&HashSet<String>>) -> Vec<Game> {
    let mut seen = HashSet::new();
    let mut games: Vec<Game> = items
        .iter()
        .filter(|i| worth_fetching(i))
        .filter(|i| seen.insert(i.catalog_item_id.clone()))
        .filter_map(|i| {
            let app_name = i.app_name.clone().unwrap_or_default();
            let (title, cover_url, launch) = match catalog.get(&i.catalog_item_id) {
                Some(info) => {
                    if info.main_game_item.is_some() || info.categories.iter().any(|c| c.path == "mods") {
                        return None;
                    }
                    (info.title.clone(), pick_cover(&info.key_images), info.launch_info())
                }
                None => (app_name.clone(), None, LaunchInfo::default()),
            };
            let installable = windows_apps.is_none_or(|apps| apps.contains(&app_name));
            Some(Game { app_name, namespace: i.namespace.clone(), catalog_item_id: i.catalog_item_id.clone(), title, cover_url, installable, launch })
        })
        .collect();
    let twins: HashSet<(String, String)> =
        games.iter().filter(|g| g.installable).map(|g| (g.namespace.clone(), g.title.to_lowercase())).collect();
    games.retain(|g| g.installable || !twins.contains(&(g.namespace.clone(), g.title.to_lowercase())));
    games.sort_by_key(|g| g.title.to_lowercase());
    games
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn item(ns: &str, id: &str, app: Option<&str>, sandbox: Option<&str>) -> LibraryItem {
        LibraryItem {
            namespace: ns.into(),
            catalog_item_id: id.into(),
            app_name: app.map(Into::into),
            sandbox_type: sandbox.map(Into::into),
        }
    }

    fn info(title: &str, images: &[(&str, &str)], categories: &[&str], dlc: bool) -> CatalogItem {
        CatalogItem {
            title: title.into(),
            key_images: images.iter().map(|(k, u)| KeyImage { kind: (*k).into(), url: (*u).into() }).collect(),
            categories: categories.iter().map(|p| Category { path: (*p).into() }).collect(),
            main_game_item: dlc.then(|| serde_json::json!({"id": "main"})),
            custom_attributes: None,
        }
    }

    #[test]
    fn launch_info_reads_catalog_attributes() {
        let item: CatalogItem = serde_json::from_value(serde_json::json!({
            "title": "Reef",
            "customAttributes": {
                "AdditionalCommandLine": {"type": "STRING", "value": " -nosplash "},
                "OwnershipToken": {"type": "STRING", "value": "True"},
                "CanRunOffline": {"type": "STRING", "value": "false"},
                "ThirdPartyManagedApp": {"type": "STRING", "value": "Origin"}
            }
        }))
        .unwrap();
        assert_eq!(
            item.launch_info(),
            LaunchInfo {
                additional_command_line: Some("-nosplash".into()),
                requires_ot: true,
                can_run_offline: false,
                third_party_store: Some("Origin".into()),
            }
        );
    }

    #[test]
    fn launch_info_defaults_without_attributes() {
        let d = info("X", &[], &[], false).launch_info();
        assert_eq!(d, LaunchInfo::default());
        assert!(d.can_run_offline && !d.requires_ot && d.additional_command_line.is_none() && d.third_party_store.is_none());
        let provider: CatalogItem = serde_json::from_value(serde_json::json!({
            "title": "X", "customAttributes": {"ThirdPartyManagedProvider": {"value": "Ubisoft Connect"}, "AdditionalCommandLine": {"value": "  "}}
        }))
        .unwrap();
        assert_eq!(provider.launch_info().third_party_store.as_deref(), Some("Ubisoft Connect"));
        assert_eq!(provider.launch_info().additional_command_line, None);
    }

    #[test]
    fn build_library_keeps_launch_info() {
        let mut catalog = HashMap::new();
        let mut c = info("Reef", &[], &[], false);
        c.custom_attributes = Some(serde_json::json!({"OwnershipToken": {"value": "true"}}));
        catalog.insert("c1".to_string(), c);
        let games = build_library(&[item("ns", "c1", Some("Reef"), None), item("ns", "c2", Some("Other"), None)], &catalog, None);
        assert!(games.iter().find(|g| g.app_name == "Reef").unwrap().launch.requires_ot);
        assert_eq!(games.iter().find(|g| g.app_name == "Other").unwrap().launch, LaunchInfo::default());
    }

    #[test]
    fn old_cached_game_without_launch_loads_with_defaults() {
        let g: Game = serde_json::from_value(serde_json::json!({
            "app_name": "A", "namespace": "ns", "catalog_item_id": "c", "title": "A", "cover_url": null, "installable": true
        }))
        .unwrap();
        assert_eq!(g.launch, LaunchInfo::default());
    }

    #[test]
    fn folder_name_from_custom_attributes() {
        let item: CatalogItem = serde_json::from_value(serde_json::json!({
            "title": "Tidepool Odyssey",
            "customAttributes": {"FolderName": {"type": "STRING", "value": " TidepoolOdyssey "}}
        }))
        .unwrap();
        assert_eq!(item.folder_name().as_deref(), Some("TidepoolOdyssey"));
        let blank: CatalogItem = serde_json::from_value(serde_json::json!({"title": "X", "customAttributes": {"FolderName": {"value": "  "}}})).unwrap();
        assert_eq!(blank.folder_name(), None);
        assert_eq!(info("X", &[], &[], false).folder_name(), None);
    }

    #[test]
    fn worth_fetching_skips_private_ue_and_nameless_items() {
        assert!(worth_fetching(&item("ns", "a", Some("Game"), Some("PUBLIC"))));
        assert!(!worth_fetching(&item("ns", "a", Some("Game"), Some("PRIVATE"))));
        assert!(!worth_fetching(&item("ue", "a", Some("Asset"), None)));
        assert!(!worth_fetching(&item("ns", "a", None, None)));
        assert!(!worth_fetching(&item("ns", "a", Some("1"), None)));
    }

    #[test]
    fn build_library_drops_dlc_and_mods_and_sorts_by_title() {
        let items = vec![
            item("n1", "zeta", Some("Zeta"), None),
            item("n2", "alpha", Some("Alpha"), None),
            item("n3", "dlc", Some("SomeDlc"), None),
            item("n4", "mod", Some("SomeMod"), None),
        ];
        let mut catalog = HashMap::new();
        catalog.insert("zeta".to_string(), info("Zeta Quest", &[], &["games"], false));
        catalog.insert("alpha".to_string(), info("alpha odyssey", &[], &["games"], false));
        catalog.insert("dlc".to_string(), info("Zeta Quest: Extra", &[], &["addons"], true));
        catalog.insert("mod".to_string(), info("A Mod", &[], &["mods"], false));
        let titles: Vec<String> = build_library(&items, &catalog, None).into_iter().map(|g| g.title).collect();
        assert_eq!(titles, vec!["alpha odyssey", "Zeta Quest"]);
    }

    fn apps(names: &[&str]) -> HashSet<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn games_missing_from_the_windows_list_are_not_installable() {
        let items = vec![item("n1", "a", Some("AppA"), None), item("n2", "b", Some("AppB"), None)];
        let games = build_library(&items, &HashMap::new(), Some(&apps(&["AppA"])));
        let flags: Vec<(&str, bool)> = games.iter().map(|g| (g.app_name.as_str(), g.installable)).collect();
        assert_eq!(flags, vec![("AppA", true), ("AppB", false)]);
    }

    #[test]
    fn without_a_windows_list_every_game_counts_as_installable() {
        let items = vec![item("n1", "a", Some("AppA"), None)];
        assert!(build_library(&items, &HashMap::new(), None)[0].installable);
    }

    #[test]
    fn a_non_windows_twin_of_an_installable_game_is_hidden() {
        // Like Bendy: same namespace and title twice, only one has a Windows build.
        let items = vec![
            item("bendy", "old", Some("BendyOld"), None),
            item("bendy", "new", Some("BendyNew"), None),
            item("other", "solo", Some("MacOnly"), None),
        ];
        let mut catalog = HashMap::new();
        catalog.insert("old".to_string(), info("Bendy and the Ink Machine", &[], &["games"], false));
        catalog.insert("new".to_string(), info("Bendy and the Ink Machine", &[], &["games"], false));
        catalog.insert("solo".to_string(), info("Mac Only Game", &[], &["games"], false));
        let games = build_library(&items, &catalog, Some(&apps(&["BendyNew"])));
        let shown: Vec<(&str, bool)> = games.iter().map(|g| (g.app_name.as_str(), g.installable)).collect();
        // The lone non-Windows game stays (shown as "Not on Windows"); the Bendy twin goes.
        assert_eq!(shown, vec![("BendyNew", true), ("MacOnly", false)]);
    }

    #[test]
    fn old_cached_games_without_the_flag_count_as_installable() {
        let g: Game = serde_json::from_value(serde_json::json!({
            "app_name": "A", "namespace": "n", "catalog_item_id": "a", "title": "A", "cover_url": null
        }))
        .unwrap();
        assert!(g.installable);
    }

    #[test]
    fn build_library_removes_duplicates() {
        let items = vec![item("n", "same", Some("A"), None), item("n", "same", Some("A"), None)];
        let mut catalog = HashMap::new();
        catalog.insert("same".to_string(), info("Same Game", &[], &[], false));
        assert_eq!(build_library(&items, &catalog, None).len(), 1);
    }

    #[test]
    fn missing_catalog_info_keeps_the_game_with_its_app_name() {
        let items = vec![item("n", "x", Some("Fallback"), None)];
        let games = build_library(&items, &HashMap::new(), None);
        assert_eq!(
            games,
            vec![Game {
                app_name: "Fallback".into(),
                namespace: "n".into(),
                catalog_item_id: "x".into(),
                title: "Fallback".into(),
                cover_url: None,
                installable: true,
                launch: LaunchInfo::default(),
            }]
        );
    }

    #[test]
    fn cover_prefers_tall_box_art() {
        let c = info(
            "T",
            &[("Thumbnail", "https://cdn/thumb.jpg"), ("DieselGameBoxTall", "https://cdn/tall.jpg"), ("OfferImageTall", "https://cdn/offer.jpg")],
            &[],
            false,
        );
        assert_eq!(pick_cover(&c.key_images), Some("https://cdn/tall.jpg".into()));
        assert_eq!(pick_cover(&c.key_images[..1]), Some("https://cdn/thumb.jpg".into()));
        assert_eq!(pick_cover(&[]), None);
    }
}
