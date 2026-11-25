//! A game's Epic achievements and which ones the player unlocked (spec §6q).
use crate::auth::client::{http_client, network, read_json};
use crate::auth::{AuthError, Secret};
use crate::retry::with_retries;
use reqwest::Client;
use serde::Serialize;
use serde_json::Value;

use crate::epic::{STORE_GQL_HOST, STORE_USER_AGENT};
const LOCALE: &str = "en-US";

/// Legendary's `egl_game_achievements_query`, trimmed to the fields used.
const GAME_QUERY: &str = "query Achievement($sandboxId: String!, $locale: String!) { Achievement { productAchievementsRecordBySandbox(sandboxId: $sandboxId, locale: $locale) { totalAchievements achievements { achievement { name hidden unlockedDisplayName lockedDisplayName unlockedDescription lockedDescription unlockedIconLink lockedIconLink rarity { percent } } } } } }";
/// Legendary's `egl_game_achievements_user_query`, trimmed to the fields used.
const PLAYER_QUERY: &str = "query PlayerAchievement($epicAccountId: String!, $sandboxId: String!) { PlayerAchievement { playerAchievementGameRecordsBySandbox(epicAccountId: $epicAccountId, sandboxId: $sandboxId) { records { totalUnlocked playerAchievements { playerAchievement { achievementName unlocked unlockDate } } } } } }";

pub const HIDDEN_TITLE: &str = "Hidden achievement";
pub const HIDDEN_DESCRIPTION: &str = "Keep playing to find out.";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Achievement {
    pub name: String,
    pub title: String,
    pub description: String,
    pub icon_url: Option<String>,
    pub unlocked: bool,
    pub unlocked_at: Option<String>,
    pub hidden: bool,
    pub rarity: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Achievements {
    pub total: u32,
    pub unlocked: u32,
    pub items: Vec<Achievement>,
}

/// Merges the game's list with the player's record. Inputs are whole reply bodies.
pub fn merge(game: &Value, player: &Value) -> Achievements {
    let text = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    // Unlocked rows only: Epic also sends locked ones that have progress.
    let mut unlocked: std::collections::HashMap<String, Option<String>> = std::collections::HashMap::new();
    let rows = player["data"]["PlayerAchievement"]["playerAchievementGameRecordsBySandbox"]["records"][0]["playerAchievements"].as_array();
    for r in rows.into_iter().flatten() {
        let r = &r["playerAchievement"];
        if r["unlocked"].as_bool() == Some(true) {
            unlocked.insert(text(r, "achievementName"), r["unlockDate"].as_str().map(str::to_string));
        }
    }
    let list = game["data"]["Achievement"]["productAchievementsRecordBySandbox"]["achievements"].as_array();
    let mut items: Vec<Achievement> = list
        .into_iter()
        .flatten()
        .map(|a| &a["achievement"])
        // A row without a name can't be matched or listed (and would repeat the page's keys).
        .filter(|a| a.is_object() && a["name"].as_str().is_some_and(|n| !n.is_empty()))
        .map(|a| {
            let name = text(a, "name");
            let hidden = a["hidden"].as_bool().unwrap_or(false);
            let got = unlocked.get(&name);
            let icon = |k: &str| a.get(k).and_then(Value::as_str).map(str::to_string);
            let (title, description, icon_url) = match got {
                Some(_) => (text(a, "unlockedDisplayName"), text(a, "unlockedDescription"), icon("unlockedIconLink")),
                None if hidden => (HIDDEN_TITLE.to_string(), HIDDEN_DESCRIPTION.to_string(), icon("lockedIconLink")),
                None => (text(a, "lockedDisplayName"), text(a, "lockedDescription"), icon("lockedIconLink")),
            };
            Achievement {
                name,
                title,
                description,
                icon_url,
                unlocked: got.is_some(),
                unlocked_at: got.cloned().flatten(),
                hidden,
                rarity: a["rarity"]["percent"].as_f64(),
            }
        })
        .collect();
    items.sort_by(|x, y| {
        use std::cmp::Ordering;
        let by_group = y.unlocked.cmp(&x.unlocked);
        let within = if x.unlocked {
            // Newest first; no date last.
            match (&x.unlocked_at, &y.unlocked_at) {
                (Some(a), Some(b)) => b.cmp(a),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            }
        } else {
            // Most common first; no rarity last.
            match (x.rarity, y.rarity) {
                (Some(a), Some(b)) => b.partial_cmp(&a).unwrap_or(Ordering::Equal),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            }
        };
        by_group.then(within).then_with(|| x.title.cmp(&y.title))
    });
    Achievements { total: items.len() as u32, unlocked: items.iter().filter(|i| i.unlocked).count() as u32, items }
}

/// Asks Epic's store GraphQL for achievements.
#[derive(Clone)]
pub struct AchievementsClient {
    http: Client,
    base: String,
    retry_base: std::time::Duration,
}

impl Default for AchievementsClient {
    fn default() -> Self {
        Self::new()
    }
}

impl AchievementsClient {
    pub fn new() -> Self {
        Self::with_base_url(STORE_GQL_HOST)
    }

    /// For tests: a fake Epic server.
    pub fn with_base_url(url: &str) -> Self {
        Self { http: http_client(), base: url.trim_end_matches('/').to_string(), retry_base: crate::retry::RETRY_BASE }
    }

    /// For tests: shorter waits between retries.
    pub fn with_retry_base(mut self, d: std::time::Duration) -> Self {
        self.retry_base = d;
        self
    }

    pub async fn fetch(&self, token: &Secret, account_id: &str, namespace: &str) -> Result<Achievements, AuthError> {
        let game = self.ask(token, GAME_QUERY, serde_json::json!({"sandboxId": namespace, "locale": LOCALE})).await?;
        let player = self.ask(token, PLAYER_QUERY, serde_json::json!({"sandboxId": namespace, "epicAccountId": account_id})).await?;
        Ok(merge(&game, &player))
    }

    async fn ask(&self, token: &Secret, query: &str, variables: Value) -> Result<Value, AuthError> {
        let body = serde_json::json!({"query": query, "variables": variables});
        let json = with_retries("achievements", self.retry_base, || async {
            let res = self
                .http
                .post(format!("{}/graphql", self.base))
                .header("Authorization", format!("bearer {}", token.expose()))
                .header("User-Agent", STORE_USER_AGENT)
                .json(&body)
                .send()
                .await
                .map_err(network)?;
            read_json(res).await
        })
        .await?;
        // Epic's error text is never kept (spec §12).
        if json.get("errors").is_some_and(|e| !e.is_null()) {
            return Err(AuthError::Unexpected("achievements reply had errors".into()));
        }
        // A 200 without data (a broken or HTML reply) would look like "no achievements".
        if !json["data"].is_object() {
            return Err(AuthError::Unexpected("achievements reply had no data".into()));
        }
        Ok(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{body_string_contains, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn ach(name: &str, hidden: bool, rarity: Option<f64>) -> Value {
        json!({"achievement": {
            "name": name, "hidden": hidden,
            "unlockedDisplayName": format!("{name} won"), "lockedDisplayName": format!("{name} locked"),
            "unlockedDescription": format!("{name} did it"), "lockedDescription": format!("{name} todo"),
            "unlockedIconLink": format!("https://i/{name}-on.png"), "lockedIconLink": format!("https://i/{name}-off.png"),
            "rarity": rarity.map(|p| json!({"percent": p})).unwrap_or(Value::Null)
        }})
    }
    fn game(list: Vec<Value>) -> Value {
        json!({"data": {"Achievement": {"productAchievementsRecordBySandbox": {"totalAchievements": 999, "achievements": list}}}})
    }
    fn row(name: &str, unlocked: bool, date: Option<&str>) -> Value {
        json!({"playerAchievement": {"achievementName": name, "unlocked": unlocked, "unlockDate": date}})
    }
    fn player(rows: Vec<Value>) -> Value {
        json!({"data": {"PlayerAchievement": {"playerAchievementGameRecordsBySandbox": {"records": [{"totalUnlocked": 77, "playerAchievements": rows}]}}}})
    }
    fn names(a: &Achievements) -> Vec<&str> {
        a.items.iter().map(|i| i.name.as_str()).collect()
    }
    fn fast(server: &MockServer) -> AchievementsClient {
        AchievementsClient::with_base_url(&server.uri()).with_retry_base(std::time::Duration::from_millis(1))
    }

    #[test]
    fn counts_and_unlocked_rows_only() {
        let a = merge(
            &game(vec![ach("a", false, None), ach("b", false, None), ach("c", false, None)]),
            &player(vec![row("a", true, Some("2025-01-01T00:00:00.000Z")), row("b", false, None)]),
        );
        assert_eq!((a.total, a.unlocked), (3, 1));
        assert!(a.items.iter().find(|i| i.name == "a").unwrap().unlocked);
        assert!(!a.items.iter().find(|i| i.name == "b").unwrap().unlocked);
    }

    #[test]
    fn unknown_player_rows_are_ignored() {
        let a = merge(&game(vec![ach("a", false, None)]), &player(vec![row("zzz", true, None)]));
        assert_eq!((a.total, a.unlocked), (1, 0));
    }

    #[test]
    fn hidden_locked_is_masked_hidden_unlocked_is_not() {
        let a = merge(&game(vec![ach("h1", true, None), ach("h2", true, None)]), &player(vec![row("h2", true, None)]));
        let h1 = a.items.iter().find(|i| i.name == "h1").unwrap();
        assert_eq!((h1.title.as_str(), h1.description.as_str()), (HIDDEN_TITLE, HIDDEN_DESCRIPTION));
        assert_eq!(h1.icon_url.as_deref(), Some("https://i/h1-off.png"));
        let h2 = a.items.iter().find(|i| i.name == "h2").unwrap();
        assert_eq!((h2.title.as_str(), h2.description.as_str()), ("h2 won", "h2 did it"));
    }

    #[test]
    fn locked_uses_locked_texts_unlocked_uses_unlocked_texts() {
        let a = merge(&game(vec![ach("a", false, None), ach("b", false, None)]), &player(vec![row("a", true, None)]));
        let x = a.items.iter().find(|i| i.name == "a").unwrap();
        assert_eq!((x.title.as_str(), x.description.as_str(), x.icon_url.as_deref()), ("a won", "a did it", Some("https://i/a-on.png")));
        let y = a.items.iter().find(|i| i.name == "b").unwrap();
        assert_eq!((y.title.as_str(), y.description.as_str(), y.icon_url.as_deref()), ("b locked", "b todo", Some("https://i/b-off.png")));
    }

    #[test]
    fn sorted_unlocked_newest_first_then_locked_most_common_first() {
        let a = merge(
            &game(vec![
                ach("old", false, None),
                ach("new", false, None),
                ach("nodate", false, None),
                ach("rare", false, Some(4.2)),
                ach("common", false, Some(60.0)),
                ach("norarity", false, None),
                ach("rare2", false, Some(4.2)),
            ]),
            &player(vec![
                row("old", true, Some("2024-05-01T00:00:00.000Z")),
                row("nodate", true, None),
                row("new", true, Some("2025-05-01T00:00:00.000Z")),
            ]),
        );
        assert_eq!(names(&a), vec!["new", "old", "nodate", "common", "rare", "rare2", "norarity"]);
        assert_eq!(a.items[0].unlocked_at.as_deref(), Some("2025-05-01T00:00:00.000Z"));
        assert_eq!(a.items[3].rarity, Some(60.0));
    }

    #[test]
    fn null_game_record_means_none() {
        let a = merge(&json!({"data": {"Achievement": {"productAchievementsRecordBySandbox": null}}}), &player(vec![row("a", true, None)]));
        assert_eq!(a, Achievements { total: 0, unlocked: 0, items: vec![] });
    }

    #[test]
    fn null_player_records_means_nothing_unlocked() {
        let p = json!({"data": {"PlayerAchievement": {"playerAchievementGameRecordsBySandbox": {"records": null}}}});
        let a = merge(&game(vec![ach("a", false, None), ach("b", false, None)]), &p);
        assert_eq!((a.total, a.unlocked), (2, 0));
    }

    #[tokio::test]
    async fn fetch_sends_both_queries_with_bearer_and_namespace() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .and(header("authorization", "bearer eg1~A"))
            .and(body_string_contains("productAchievementsRecordBySandbox"))
            .and(body_string_contains("ns1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(game(vec![ach("a", false, None), ach("b", false, None)])))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .and(header("authorization", "bearer eg1~A"))
            .and(body_string_contains("playerAchievementGameRecordsBySandbox"))
            .and(body_string_contains("ns1"))
            .and(body_string_contains("acc1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(player(vec![row("a", true, None)])))
            .expect(1)
            .mount(&server)
            .await;
        let a = fast(&server).fetch(&Secret::new("eg1~A"), "acc1", "ns1").await.unwrap();
        assert_eq!((a.total, a.unlocked), (2, 1));
    }

    #[tokio::test]
    async fn graphql_errors_are_an_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"errors": [{"message": "x"}], "data": null})))
            .mount(&server)
            .await;
        assert!(fast(&server).fetch(&Secret::new("eg1~A"), "acc1", "ns1").await.is_err());
    }

    #[tokio::test]
    async fn a_reply_without_data_is_an_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).respond_with(ResponseTemplate::new(200).set_body_string("<html>oops</html>")).mount(&server).await;
        assert!(fast(&server).fetch(&Secret::new("eg1~A"), "acc1", "ns1").await.is_err());
    }

    #[test]
    fn rows_without_a_name_are_dropped() {
        let a = merge(&game(vec![ach("", false, None), ach("", false, None), ach("a", false, None)]), &player(vec![]));
        assert_eq!(names(&a), vec!["a"]);
        assert_eq!(a.total, 1);
    }

    #[tokio::test]
    async fn http_error_is_an_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).respond_with(ResponseTemplate::new(403)).mount(&server).await;
        assert!(fast(&server).fetch(&Secret::new("eg1~A"), "acc1", "ns1").await.is_err());
    }
}
