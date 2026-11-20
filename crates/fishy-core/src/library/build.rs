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
}

/// A game shown in the Library (also what the cache stores).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Game {
    pub app_name: String,
    pub namespace: String,
    pub catalog_item_id: String,
    pub title: String,
    pub cover_url: Option<String>,
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
pub fn build_library(items: &[LibraryItem], catalog: &HashMap<String, CatalogItem>) -> Vec<Game> {
    let mut seen = HashSet::new();
    let mut games: Vec<Game> = items
        .iter()
        .filter(|i| worth_fetching(i))
        .filter(|i| seen.insert(i.catalog_item_id.clone()))
        .filter_map(|i| {
            let app_name = i.app_name.clone().unwrap_or_default();
            let (title, cover_url) = match catalog.get(&i.catalog_item_id) {
                Some(info) => {
                    if info.main_game_item.is_some() || info.categories.iter().any(|c| c.path == "mods") {
                        return None;
                    }
                    (info.title.clone(), pick_cover(&info.key_images))
                }
                None => (app_name.clone(), None),
            };
            Some(Game { app_name, namespace: i.namespace.clone(), catalog_item_id: i.catalog_item_id.clone(), title, cover_url })
        })
        .collect();
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
        let titles: Vec<String> = build_library(&items, &catalog).into_iter().map(|g| g.title).collect();
        assert_eq!(titles, vec!["alpha odyssey", "Zeta Quest"]);
    }

    #[test]
    fn build_library_removes_duplicates() {
        let items = vec![item("n", "same", Some("A"), None), item("n", "same", Some("A"), None)];
        let mut catalog = HashMap::new();
        catalog.insert("same".to_string(), info("Same Game", &[], &[], false));
        assert_eq!(build_library(&items, &catalog).len(), 1);
    }

    #[test]
    fn missing_catalog_info_keeps_the_game_with_its_app_name() {
        let items = vec![item("n", "x", Some("Fallback"), None)];
        let games = build_library(&items, &HashMap::new());
        assert_eq!(
            games,
            vec![Game {
                app_name: "Fallback".into(),
                namespace: "n".into(),
                catalog_item_id: "x".into(),
                title: "Fallback".into(),
                cover_url: None,
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
