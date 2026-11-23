//! The Store (spec §6o): Epic's public list of free games, and which pages the Store window may show.
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use std::time::Duration;

/// Epic's public promotions list: no sign-in, no country sent.
pub const FREE_GAMES_URL: &str = "https://store-site-backend-static-ipv4.ak.epicgames.com/freeGamesPromotions?locale=en-US";
/// Where "Open Epic's store" goes.
pub const STORE_HOME: &str = "https://store.epicgames.com/";
const FREE_GAMES_PAGE: &str = "https://store.epicgames.com/free-games";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FreeGame {
    pub title: String,
    pub url: String,
    pub image: Option<String>,
    /// Free until (free now) or free from (free next).
    pub date: DateTime<Utc>,
    pub owned: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct FreeGames {
    pub now: Vec<FreeGame>,
    pub next: Vec<FreeGame>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreError {
    Network,
    Format,
}

fn date(v: &Value, key: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(v.get(key)?.as_str()?).ok().map(|d| d.with_timezone(&Utc))
}

/// The 0% offers ("free") of one kind, as (start, end).
fn free_offers(e: &Value, kind: &str) -> Vec<(DateTime<Utc>, DateTime<Utc>)> {
    let groups = e.get("promotions").and_then(|p| p.get(kind)).and_then(Value::as_array).cloned().unwrap_or_default();
    groups
        .iter()
        .filter_map(|g| g.get("promotionalOffers").and_then(Value::as_array))
        .flatten()
        .filter(|o| o.get("discountSetting").and_then(|d| d.get("discountPercentage")).and_then(Value::as_f64) == Some(0.0))
        .filter_map(|o| Some((date(o, "startDate")?, date(o, "endDate")?)))
        .collect()
}

fn page_url(e: &Value) -> String {
    let slug = e
        .get("productSlug")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(|s| s.trim_end_matches("/home").to_string())
        .or_else(|| {
            let maps = e.get("catalogNs")?.get("mappings")?.as_array()?;
            maps.iter().find(|m| m.get("pageType").and_then(Value::as_str) == Some("productHome"))?.get("pageSlug")?.as_str().map(str::to_string)
        })
        .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '/'));
    slug.map_or_else(|| FREE_GAMES_PAGE.to_string(), |s| format!("https://store.epicgames.com/p/{s}"))
}

fn cover(e: &Value) -> Option<String> {
    let images = e.get("keyImages")?.as_array()?;
    let of = |kind: &str| {
        images.iter().find(|i| i.get("type").and_then(Value::as_str) == Some(kind)).and_then(|i| i.get("url")?.as_str()).filter(|u| u.starts_with("https://")).map(str::to_string)
    };
    of("OfferImageWide").or_else(|| of("Thumbnail"))
}

fn owned(e: &Value, ids: &HashSet<String>) -> bool {
    e.get("items").and_then(Value::as_array).is_some_and(|items| items.iter().any(|i| i.get("id").and_then(Value::as_str).is_some_and(|id| ids.contains(id))))
}

/// Free now (a 0% offer running at `now`) and free next (an upcoming 0% offer), each sorted by
/// date, then title. Other discounts are left out.
pub fn parse_free_games(json: &[u8], now: DateTime<Utc>, owned_catalog_ids: &HashSet<String>) -> Result<FreeGames, StoreError> {
    let v: Value = serde_json::from_slice(json).map_err(|_| StoreError::Format)?;
    let elements = v.pointer("/data/Catalog/searchStore/elements").and_then(Value::as_array).ok_or(StoreError::Format)?;
    let mut out = FreeGames::default();
    for e in elements {
        let Some(title) = e.get("title").and_then(Value::as_str) else { continue };
        let game = |date| FreeGame { title: title.to_string(), url: page_url(e), image: cover(e), date, owned: owned(e, owned_catalog_ids) };
        // An "upcoming" offer that has already begun counts as free now: Epic's copy lags at the switch-over.
        let running = |(start, end): &(DateTime<Utc>, DateTime<Utc>)| *start <= now && now < *end;
        let current = free_offers(e, "promotionalOffers").into_iter().find(running).or_else(|| free_offers(e, "upcomingPromotionalOffers").into_iter().find(running));
        if let Some((_, end)) = current {
            out.now.push(game(end));
        } else if let Some(start) = free_offers(e, "upcomingPromotionalOffers").into_iter().filter(|(_, end)| now < *end).map(|(s, _)| s).min() {
            out.next.push(game(start));
        }
    }
    for list in [&mut out.now, &mut out.next] {
        list.sort_by(|a, b| a.date.cmp(&b.date).then_with(|| a.title.cmp(&b.title)));
    }
    Ok(out)
}

/// Pages the Store window shows itself: https on epicgames.com or its subdomains.
pub fn store_nav_allowed(url: &str) -> bool {
    let Ok(u) = url::Url::parse(url) else { return false };
    u.scheme() == "https" && u.host_str().is_some_and(|h| h == "epicgames.com" || h.ends_with(".epicgames.com"))
}

/// Fetches Epic's promotions list.
pub struct StoreClient {
    http: reqwest::Client,
    url: String,
}

impl Default for StoreClient {
    fn default() -> Self {
        Self::new()
    }
}

impl StoreClient {
    pub fn new() -> Self {
        Self::with_url(FREE_GAMES_URL)
    }

    /// For tests: a fake server.
    pub fn with_url(url: &str) -> Self {
        Self { http: reqwest::Client::builder().timeout(Duration::from_secs(20)).build().expect("http client"), url: url.to_string() }
    }

    pub async fn free_games_json(&self) -> Result<Vec<u8>, StoreError> {
        let res = self.http.get(&self.url).send().await.map_err(|_| StoreError::Network)?;
        if !res.status().is_success() {
            return Err(StoreError::Network);
        }
        res.bytes().await.map(|b| b.to_vec()).map_err(|_| StoreError::Network)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const REAL: &str = include_str!("../testdata/free-games.json");

    fn at(y: i32, m: u32, d: u32, h: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, h, 0, 0).unwrap()
    }

    fn titles(list: &[FreeGame]) -> Vec<&str> {
        list.iter().map(|g| g.title.as_str()).collect()
    }

    fn offer(extra: serde_json::Value) -> Vec<u8> {
        let mut e = serde_json::json!({
            "title": "Tide",
            "promotions": { "promotionalOffers": [{ "promotionalOffers": [{
                "startDate": "2026-11-20T16:00:00.000Z", "endDate": "2026-11-30T16:00:00.000Z",
                "discountSetting": { "discountType": "PERCENTAGE", "discountPercentage": 0 }
            }]}], "upcomingPromotionalOffers": [] }
        });
        for (k, v) in extra.as_object().unwrap() {
            e[k] = v.clone();
        }
        serde_json::to_vec(&serde_json::json!({ "data": { "Catalog": { "searchStore": { "elements": [e] } } } })).unwrap()
    }

    fn one(extra: serde_json::Value) -> FreeGame {
        parse_free_games(&offer(extra), at(2026, 11, 24, 12), &HashSet::new()).unwrap().now.remove(0)
    }

    #[test]
    fn real_response_splits_now_and_next() {
        let g = parse_free_games(REAL.as_bytes(), at(2026, 11, 24, 12), &HashSet::new()).unwrap();
        assert_eq!(titles(&g.now), vec!["Mark of the Deep"]);
        assert_eq!(g.now[0].date, at(2026, 11, 26, 16));
        assert_eq!(g.now[0].url, "https://store.epicgames.com/p/mark-of-the-deep-f7dc08");
        assert_eq!(g.next.len(), 2);
        assert!(g.next[0].title.starts_with("Bread and Fred"), "{:?}", titles(&g.next));
        assert!(g.next[1].title.starts_with("Firestone"));
        assert_eq!(g.next[0].date, at(2026, 11, 26, 16));
        assert!(!titles(&g.now).contains(&"TerraScape"), "70% off isn't free");
    }

    #[test]
    fn ended_offer_is_left_out() {
        let g = parse_free_games(REAL.as_bytes(), at(2026, 11, 27, 0), &HashSet::new()).unwrap();
        assert!(!titles(&g.now).contains(&"Mark of the Deep"), "its offer ended on Nov 26");
    }

    #[test]
    fn a_free_offer_beside_a_discount_counts_by_the_free_one() {
        let json = offer(serde_json::json!({ "promotions": { "promotionalOffers": [], "upcomingPromotionalOffers": [{ "promotionalOffers": [
            { "startDate": "2026-12-10T16:00:00.000Z", "endDate": "2027-01-07T16:00:00.000Z", "discountSetting": { "discountPercentage": 40 } },
            { "startDate": "2026-11-26T16:00:00.000Z", "endDate": "2026-12-03T16:00:00.000Z", "discountSetting": { "discountPercentage": 0 } }
        ]}]}}));
        let g = parse_free_games(&json, at(2026, 11, 24, 12), &HashSet::new()).unwrap();
        assert_eq!(g.next[0].date, at(2026, 11, 26, 16));
    }

    #[test]
    fn an_upcoming_offer_that_already_started_is_free_now() {
        // Epic's copy often lags at the switch-over: the "upcoming" offer has begun.
        let json = offer(serde_json::json!({ "promotions": { "promotionalOffers": [], "upcomingPromotionalOffers": [{ "promotionalOffers": [
            { "startDate": "2026-11-24T10:00:00.000Z", "endDate": "2026-12-01T16:00:00.000Z", "discountSetting": { "discountPercentage": 0 } }
        ]}]}}));
        let g = parse_free_games(&json, at(2026, 11, 24, 12), &HashSet::new()).unwrap();
        assert_eq!(g.now[0].date, at(2026, 12, 1, 16));
        assert!(g.next.is_empty());
    }

    #[test]
    fn a_zero_written_as_0_0_is_free() {
        let json = offer(serde_json::json!({ "promotions": { "promotionalOffers": [{ "promotionalOffers": [
            { "startDate": "2026-11-20T16:00:00.000Z", "endDate": "2026-11-30T16:00:00.000Z", "discountSetting": { "discountPercentage": 0.0 } }
        ]}], "upcomingPromotionalOffers": [] }}));
        assert_eq!(parse_free_games(&json, at(2026, 11, 24, 12), &HashSet::new()).unwrap().now.len(), 1);
    }

    #[test]
    fn slug_rules() {
        assert_eq!(one(serde_json::json!({ "productSlug": "thems-fightin-herds/home" })).url, "https://store.epicgames.com/p/thems-fightin-herds");
        let mapped = serde_json::json!({ "productSlug": null, "catalogNs": { "mappings": [{ "pageSlug": "x-old", "pageType": "offer" }, { "pageSlug": "tide-1a2b", "pageType": "productHome" }] } });
        assert_eq!(one(mapped).url, "https://store.epicgames.com/p/tide-1a2b");
        assert_eq!(one(serde_json::json!({})).url, "https://store.epicgames.com/free-games");
    }

    #[test]
    fn cover_prefers_wide_then_thumbnail() {
        let both = serde_json::json!({ "keyImages": [{ "type": "Thumbnail", "url": "https://cdn/t.jpg" }, { "type": "OfferImageWide", "url": "https://cdn/w.jpg" }] });
        assert_eq!(one(both).image.as_deref(), Some("https://cdn/w.jpg"));
        assert_eq!(one(serde_json::json!({ "keyImages": [{ "type": "Thumbnail", "url": "https://cdn/t.jpg" }] })).image.as_deref(), Some("https://cdn/t.jpg"));
        assert_eq!(one(serde_json::json!({ "keyImages": [{ "type": "OfferImageWide", "url": "http://cdn/w.jpg" }] })).image, None, "only https");
    }

    #[test]
    fn owned_by_catalog_item_id() {
        let json = offer(serde_json::json!({ "items": [{ "id": "cat1", "namespace": "ns" }] }));
        let owned: HashSet<String> = ["cat1".to_string()].into();
        assert!(parse_free_games(&json, at(2026, 11, 24, 12), &owned).unwrap().now[0].owned);
        assert!(!parse_free_games(&json, at(2026, 11, 24, 12), &HashSet::new()).unwrap().now[0].owned);
    }

    #[test]
    fn null_fields_dont_crash() {
        let json = br#"{"data":{"Catalog":{"searchStore":{"elements":[{"title":"A","promotions":null,"productSlug":null,"keyImages":[]},{"title":"B"}]}}}}"#;
        assert_eq!(parse_free_games(json, at(2026, 11, 24, 12), &HashSet::new()).unwrap(), FreeGames::default());
    }

    #[test]
    fn bad_json_is_a_format_error() {
        assert_eq!(parse_free_games(b"<html>", at(2026, 11, 24, 12), &HashSet::new()), Err(StoreError::Format));
        assert_eq!(parse_free_games(b"{}", at(2026, 11, 24, 12), &HashSet::new()), Err(StoreError::Format));
    }

    #[test]
    fn nav_rule() {
        for ok in ["https://store.epicgames.com/p/x", "https://www.epicgames.com/id/login", "https://epicgames.com/"] {
            assert!(store_nav_allowed(ok), "{ok}");
        }
        for bad in ["http://store.epicgames.com/", "https://epicgames.com.evil.net/", "https://paypal.com/", "javascript:alert(1)", "not a url"] {
            assert!(!store_nav_allowed(bad), "{bad}");
        }
    }

    #[tokio::test]
    async fn fetch_returns_the_body_and_fails_on_500() {
        let ok = MockServer::start().await;
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(200).set_body_string("{}")).mount(&ok).await;
        assert_eq!(StoreClient::with_url(&ok.uri()).free_games_json().await.unwrap(), b"{}");
        let bad = MockServer::start().await;
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(500)).mount(&bad).await;
        assert_eq!(StoreClient::with_url(&bad.uri()).free_games_json().await, Err(StoreError::Network));
    }
}
