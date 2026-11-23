# Game page and achievements Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Clicking a game's cover opens a game page with the card's buttons, size/playtime, and the game's Epic achievements (All · Unlocked · Locked).

**Architecture:** A new Rust module asks Epic's store GraphQL for the game's achievement list and the player's unlocked ones and merges them (pure, tested). A Tauri command looks the namespace up in the cached library. In the UI, `LibraryPage` keeps all button logic and dialogs and swaps the grid for a new `GamePage` that receives the buttons as a snippet.

**Tech Stack:** Rust (reqwest, serde_json, wiremock tests), Tauri 2, Svelte 5 runes, vitest + @testing-library/svelte + `mockIPC`.

**Spec:** `docs/superpowers/specs/2026-11-20-fishy-client-design.md` §6q (read it first; copy and texts come from it).

## Global Constraints

- Endpoint `https://launcher.store.epicgames.com/graphql`, `POST`, headers `Authorization: bearer <token>` and `User-Agent: EpicGamesLauncher/14.0.8-22004686+++Portal+Release-Live`, locale `en-US`.
- Queries: Legendary's `egl_game_achievements_query` and `egl_game_achievements_user_query` (`legendary/models/gql.py`), trimmed to the fields used; `sandboxId` = namespace, `epicAccountId` = saved account ID.
- Privacy (spec §12): never log the token, account ID, namespace, title or Epic's reply text. Logs are exactly "achievements loaded: U of T" and "achievements failed".
- The namespace comes only from the cached library, never from the page.
- Nothing about achievements is saved on disk; fetched each time a page opens.
- UI texts verbatim from §6q: "← Library", "Achievements", "U / T unlocked", "All", "Unlocked", "Locked", "Unlocked 12 Mar 2025", "4.2% of players have it", "Hidden achievement", "Keep playing to find out.", "Loading achievements…", "Couldn't load achievements from Epic.", "Try again", "This game has no Epic achievements.", "None unlocked yet.", "You have them all!".
- Commit messages end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Run `npm.cmd`, not `npm`.

## Review Focus

1. A player row whose `achievementName` isn't in the game's list (Epic data out of step): ignored; `unlocked` counts only matched, unlocked items, so it never shows "64 / 63". (Task 1 test `unknown_player_rows_are_ignored`.)
2. Opening game A, then quickly game B: A's late reply must not show on B's page. (Task 4 test `a_late_reply_for_another_game_is_ignored`.)
3. A hidden achievement that is **unlocked** shows its real title and description, not "Hidden achievement". (Task 1 test `hidden_locked_is_masked_hidden_unlocked_is_not`.)
4. Missing `unlockDate` or `rarity`: the row shows "Unlocked" alone / no % text, no "Invalid Date" or "NaN%". (Task 3 tests.)
5. The open game leaves the library (refresh after Log out / library change): the page closes back to the grid instead of showing a stale game. (Task 4 test `page_closes_when_the_game_leaves_the_library`.)

---

### Task 1: Rust core `achievements` module

**Files:**
- Create: `crates/fishy-core/src/achievements.rs`
- Modify: `crates/fishy-core/src/lib.rs` (add `pub mod achievements;`)

**Interfaces:**
- Consumes: `crate::auth::{Secret, AuthError}`, `crate::auth::client::{network, read_json}`, `crate::retry::with_retries` (same use as `library/client.rs`).
- Produces:
  - `#[derive(Debug, Clone, PartialEq, Serialize)] pub struct Achievement { pub name: String, pub title: String, pub description: String, pub icon_url: Option<String>, pub unlocked: bool, pub unlocked_at: Option<String>, pub hidden: bool, pub rarity: Option<f64> }`
  - `#[derive(Debug, Clone, PartialEq, Serialize)] pub struct Achievements { pub total: u32, pub unlocked: u32, pub items: Vec<Achievement> }`
  - `pub fn merge(game: &serde_json::Value, player: &serde_json::Value) -> Achievements` — inputs are the whole reply bodies (`{"data": …}`).
  - `pub struct AchievementsClient` with `pub fn new() -> Self`, `pub fn with_base_url(url: &str) -> Self`, `pub fn with_retry_base(self, d: Duration) -> Self`, `pub async fn fetch(&self, token: &Secret, account_id: &str, namespace: &str) -> Result<Achievements, AuthError>`.

- [ ] **Step 1: Write failing tests for `merge`** (in the module's `#[cfg(test)] mod tests`, with small hand-made JSON in the shapes of spec §6q):
  - `counts_and_unlocked_rows_only`: game list of 3 (a, b, c); player rows a `unlocked:true`, b `unlocked:false` with progress → `total == 3`, `unlocked == 1`, only `a.unlocked`.
  - `unknown_player_rows_are_ignored`: a player row `zzz` `unlocked:true` not in the list → `unlocked` unchanged.
  - `hidden_locked_is_masked_hidden_unlocked_is_not`: hidden locked → title "Hidden achievement", description "Keep playing to find out.", icon = locked icon link; hidden unlocked → its `unlockedDisplayName`/`unlockedDescription`.
  - `locked_uses_locked_texts_unlocked_uses_unlocked_texts` (title/description/icon pick the locked* or unlocked* fields).
  - `sorted_unlocked_newest_first_then_locked_most_common_first`: unlock dates 2024 and 2025 → 2025 first; locked rarities 4.2 and 60.0 → 60.0 first; items without date / rarity go last in their group; ties by title.
  - `null_game_record_means_none`: `{"data":{"Achievement":{"productAchievementsRecordBySandbox":null}}}` → `total 0, unlocked 0, items []`.
  - `null_player_records_means_nothing_unlocked`: `records: null` → all locked.
  - `total` = number of items in the list (not Epic's `totalAchievements`, so the bar never disagrees with the rows).
- [ ] **Step 2: Run** `cargo test -p fishy-core achievements` → FAIL (module missing).
- [ ] **Step 3: Implement `merge`** reading `data.Achievement.productAchievementsRecordBySandbox.achievements[].achievement` and `data.PlayerAchievement.playerAchievementGameRecordsBySandbox.records[0].playerAchievements[].playerAchievement` (`achievementName`, `unlocked`, `unlockDate`); `rarity` from `rarity.percent`; missing strings → "".
- [ ] **Step 4: Write failing client tests** (wiremock, like `library/client.rs` tests):
  - `fetch_sends_both_queries_with_bearer_and_namespace`: two mocks on `POST /graphql` matched by `header("authorization","bearer eg1~A")` and `body_string_contains("productAchievementsRecordBySandbox")` / `body_string_contains("playerAchievementGameRecordsBySandbox")` and `body_string_contains("ns1")`; result merged (e.g. 1 of 2).
  - `graphql_errors_are_an_error`: reply `{"errors":[{"message":"x"}],"data":null}` → `Err`.
  - `http_error_is_an_error`: 403 → `Err`.
- [ ] **Step 5: Implement `AchievementsClient::fetch`**: base URL default `https://launcher.store.epicgames.com`, path `/graphql`, JSON body `{"query": …, "variables": {…}}`, each request in `with_retries("achievements", …)` + `read_json`; a body with a non-null `errors` → `AuthError::Unexpected("achievements reply had errors".into())` (fixed text, never Epic's).
- [ ] **Step 6: Run** `cargo test -p fishy-core achievements` → PASS; `cargo clippy -p fishy-core` clean.
- [ ] **Step 7: Commit** `feat(achievements): read a game's Epic achievements (spec §6q)`.

### Task 2: Tauri command `game_achievements` + README

**Files:**
- Modify: `app/src-tauri/src/lib.rs` (state, command, `invoke_handler`)
- Modify: `README.md` ("What is sent, and to whom" table)

**Interfaces:**
- Consumes: Task 1 `AchievementsClient`, `Achievements`; `AppLibrary.cache.load()`; `auth.manager.access_token()`, `auth.manager.saved_identity()`; `session::expire`, `session::EXPIRED`.
- Produces: `#[tauri::command] async fn game_achievements(app_name: String, app, auth, library) -> Result<Achievements, String>`; helper `fn namespace_of(games: &[Game], app_name: &str) -> Option<String>`.

- [ ] **Step 1: Failing unit test** in lib.rs tests: `namespace_comes_from_the_cached_library` (`namespace_of` finds it by `app_name`; unknown → `None`).
- [ ] **Step 2: Run** `cargo test -p fishy-client namespace_comes` → FAIL.
- [ ] **Step 3: Implement**: add `achievements: AchievementsClient` to `AppLibrary` (built with `new()`); the command: namespace via `namespace_of` (None → `Err(ACHIEVEMENTS_ERROR)`), token via `access_token()` (Expired → `session::expire` + `Err(session::EXPIRED)`, like `library_refresh`), account ID via `saved_identity()`; on success `info!("achievements loaded: {} of {}", a.unlocked, a.total)`; on any failure `warn!("achievements failed")` and `Err(ACHIEVEMENTS_ERROR)` where `const ACHIEVEMENTS_ERROR: &str = "Couldn't load achievements from Epic.";`. Register in `invoke_handler`.
- [ ] **Step 4: README row** after the store-site row: `| \`launcher.store.epicgames.com\` | a game's achievements and which ones you unlocked, when you open its page (your sign-in and account ID are sent) |`.
- [ ] **Step 5: Run** `cargo test --workspace` → PASS; `cargo clippy --workspace` clean.
- [ ] **Step 6: Commit** `feat(achievements): game_achievements command (spec §6q)`.

### Task 3: `achievements.ts`

**Files:**
- Create: `app/src/lib/library/achievements.ts`, `app/src/lib/library/achievements.test.ts`

**Interfaces:**
- Produces: `type Achievement` / `type Achievements` (same fields as Rust); `type AchievementFilter = "all" | "unlocked" | "locked"`; `loadAchievements(app_name: string): Promise<Achievements>` (`invoke("game_achievements", { appName })`); `filterAchievements(items: Achievement[], f: AchievementFilter): Achievement[]`; `formatUnlocked(iso: string | null): string`; `formatRarity(p: number | null): string | null`.

- [ ] **Step 1: Failing tests**:
  - `formatUnlocked("2025-03-12T18:00:00.000Z") === "Unlocked 12 Mar 2025"`; `formatUnlocked(null) === "Unlocked"`; `formatUnlocked("garbage") === "Unlocked"`.
  - `formatRarity(4.2) === "4.2% of players have it"`; `formatRarity(60) === "60.0% of players have it"`; `formatRarity(null) === null`; `formatRarity(NaN) === null`.
  - `filterAchievements` keeps order and returns all / only unlocked / only locked.
- [ ] **Step 2: Run** `npm.cmd --prefix app test -- achievements` → FAIL.
- [ ] **Step 3: Implement** (dates with `toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric" })`, read in UTC; `toFixed(1)`).
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit** `feat(achievements): achievement helpers for the game page (spec §6q)`.

### Task 4: `GamePage.svelte` and opening it from the Library

**Files:**
- Create: `app/src/lib/library/GamePage.svelte`, `app/src/lib/library/GamePage.test.ts`
- Modify: `app/src/lib/library/LibraryPage.svelte`, `app/src/lib/library/library.test.ts`

**Interfaces:**
- Consumes: Task 3 exports; `FishLoader` (`size`), `FishProgressBar` only if its `state` fits a static bar, else a simple bar in the fish style (CSS of `theme.css`).
- Produces: `GamePage` props `{ game: Game; actions: Snippet<[Game]>; lines: string[]; onback: () => void }` (`lines` = the size and playtime texts already shown in the ⋯ menu).

- [ ] **Step 1: Failing `GamePage.test.ts`** (mock `game_achievements` with `mockIPC`):
  - `shows_count_and_rows`: 2 items, 1 unlocked → "1 / 2 unlocked", both titles, "Unlocked 12 Mar 2025", "4.2% of players have it"; the locked row has class `locked`.
  - `filter_switches_rows`: click "Unlocked" → only the unlocked title; "Locked" → only the locked one; empty Unlocked → "None unlocked yet."; empty Locked → "You have them all!".
  - `no_achievements_text`: `total 0` → "This game has no Epic achievements." and no filter.
  - `error_then_try_again`: first call rejects → "Couldn't load achievements from Epic." + "Try again"; second call resolves → rows show.
  - `a_late_reply_for_another_game_is_ignored`: render game A with a slow reply, rerender with game B (fast reply), then resolve A → only B's rows.
  - `back_calls_onback` ("← Library").
- [ ] **Step 2: Failing `library.test.ts` additions**:
  - `cover_opens_the_game_page_and_back_returns`: click button "Open Game X" → heading "Game X" and "← Library"; the page shows the card's "Install Game X" button; "← Library" → grid back with the same tab and search text.
  - `page_closes_when_the_game_leaves_the_library`: open a game, then the library store refreshes without it → grid shows.
- [ ] **Step 3: Run** `npm.cmd --prefix app test` → the new tests FAIL.
- [ ] **Step 4: Implement**: in `LibraryPage`, move the card's button row (Install/Play/Update/⋯ and the notes/errors under it) into a `{#snippet actions(game)}` used by both the card and `GamePage`; wrap the cover `<div class="cover">` content in `<button class="cover-open" aria-label="Open {game.title}">`; `let openApp = $state<string | null>(null)`, the open game derived from `$library.games` (gone → `openApp = null`); while open, render `GamePage` instead of the tabs + grid (keep header, dialogs outside); the filter resets on a new game via a keyed block. `GamePage` loads on mount / game change with a request counter to drop late replies; layout per spec §6q (cover left, title, actions, lines; Achievements section; rows icon 48 px with `loading="lazy" referrerpolicy="no-referrer"`, locked rows `opacity` faded and icon `grayscale`).
- [ ] **Step 5: Run** `npm.cmd --prefix app test` and `npm.cmd --prefix app run check` → PASS / 0 errors.
- [ ] **Step 6: Commit** `feat(library): game page with achievements (spec §6q)`.

### Task 5: Release notes and version 1.7.0

**Files:**
- Modify: `app/src/lib/notes/releaseNotes.ts` (1.7.0 entry: "Click a game's cover to open its page, with its buttons and your Epic achievements."), version files as listed in `docs/RELEASING.md`, spec §12 progress log line.

- [ ] **Step 1:** Add the notes entry; `npm.cmd --prefix app test -- notes` → PASS.
- [ ] **Step 2:** Fresh whole-branch review on the most capable model; fix findings, commit `fix(achievements): review of 1.7.0 (spec §6q)`.
- [ ] **Step 3:** Release per `docs/RELEASING.md` (version bump commit `release: version 1.7.0`, signed build from Git Bash, `latest.json`, draft then public, push `feat/import` and `master`).
- [ ] **Step 4:** Hand check with the user (spec §6q "By hand").
