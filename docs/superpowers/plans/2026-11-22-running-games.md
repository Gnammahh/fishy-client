# Running Games Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** After Play, the Library card shows **Running** until no program from the game's install folder is open. Play, Update and Uninstall are greyed out meanwhile. An optional setting minimizes Fishy Client while a game runs. Play times are saved and power a "Recently played" sort.

**Architecture:** fishy-core gets a `running` module: the folder match, a `RunningList`, and a `sysinfo`-backed list of open program paths. The app keeps one `AppRunning` state. A successful Play adds the game and starts a single 5-second loop that ends when the list is empty. Each change is sent to the UI as `running-changed`. The UI keeps a `running` store and greys out the card.

**Tech Stack:** Rust (fishy-core, Tauri 2 app, tokio, `sysinfo` 0.40), Svelte 5 + vitest.

**Spec:** `docs/superpowers/specs/2026-11-20-fishy-client-design.md` §6f.4 (and §6d Launch, §6f.3 Sort).

## Global Constraints

- Check interval: **5 seconds**, only while at least one game started by Play is in the list; the loop ends when it's empty.
- Folder match: whole folder names, capital letters ignored, `/` and `\` treated alike.
- Exact copy: card button **Running**; note **"Close *Title* first."**; Play refusal **"*Title* is already running."**; Update/Uninstall refusal **"Close *Title* first."**
- Settings copy: label **"When a game starts"**; options **"Keep Fishy Client open"** (`"keep"`, default) and **"Minimize it, and bring it back when the game closes"** (`"minimize"`).
- Sort option label **"Recently played"**, value `"played"`.
- Saved data: `settings.json` gains `when_game_starts` (missing → `"keep"`); `installed.json` records gain `last_played` (UTC, missing → none).
- Logs: only `{app_name} is running`, `{app_name} closed`, `couldn't save play time for {app_name}`. Other programs' paths are never logged, saved or sent.
- Downloads are never paused or touched by any of this.
- Commit messages end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Use `npm.cmd` in PowerShell.

## Review Focus

1. **The game's own launcher exits and the game keeps running from a subfolder.** It must still show Running, because the check uses the folder, not the first process. Test: `RunningList` keeps a game whose only program is `…\Kelp\Binaries\Win64\kelp.exe` (Task 1).
2. **Two games running, one closes.** Only that one is removed, and the window isn't restored until both have closed. Tests in Task 1 (`check`) and Task 3 (`should_restore`).
3. **Play pressed again during the 5-second "Starting…" window, or from a second click before `running-changed` arrives.** The command refuses because the game is already in the list (Task 3 `already_running` check runs before the exchange code is fetched).
4. **Install folder saved with a trailing slash or different capital letters** (`C:\games\kelp\`). It must still match. Test in Task 1.
5. **`installed.json` written by an install finishing while Play saves a play time.** Neither change may be lost. Test in Task 2 with both writes done one after another through the shared lock, checking both fields survive; and `finish_install` keeps `last_played`.

---

### Task 1: fishy-core `running` module

**Files:**
- Modify: `crates/fishy-core/Cargo.toml` (add `sysinfo = { version = "0.40", default-features = false, features = ["system"] }`)
- Create: `crates/fishy-core/src/running.rs`
- Modify: `crates/fishy-core/src/lib.rs` (add `pub mod running;`)

**Interfaces:**
- Produces:
  - `pub fn is_inside(program: &Path, folder: &Path) -> bool`
  - `pub struct RunningList` (Default) with
    - `pub fn add(&mut self, app_name: &str, folder: &Path) -> bool`: true when newly added
    - `pub fn contains(&self, app_name: &str) -> bool`
    - `pub fn names(&self) -> Vec<String>`: sorted
    - `pub fn is_empty(&self) -> bool`
    - `pub fn check(&mut self, programs: &[PathBuf]) -> Vec<String>`: removes and returns the closed games
  - `pub fn already_running(title: &str) -> String` → `"{title} is already running."`
  - `pub fn close_first(title: &str) -> String` → `"Close {title} first."`
  - `pub struct OpenPrograms` (wraps a `sysinfo::System`, kept between checks) with `pub fn new() -> Self` and `pub fn paths(&mut self) -> Vec<PathBuf>`

- [ ] **Step 1: Write failing tests** in `running.rs`:

```rust
#[test]
fn a_program_inside_the_folder_counts_a_look_alike_does_not() {
    let f = Path::new(r"C:\Games\Kelp");
    assert!(is_inside(Path::new(r"C:\Games\Kelp\kelp.exe"), f));
    assert!(is_inside(Path::new(r"c:\games\KELP\Binaries\Win64\kelp.exe"), f));
    assert!(is_inside(Path::new("C:/Games/Kelp/bin/kelp.exe"), f));
    assert!(is_inside(Path::new(r"C:\Games\Kelp\kelp.exe"), Path::new(r"C:\games\kelp\")));
    assert!(!is_inside(Path::new(r"C:\Games\Kelp2\kelp.exe"), f));
    assert!(!is_inside(Path::new(r"C:\Games\kelp.exe"), f));
    assert!(!is_inside(Path::new(r"D:\Games\Kelp\kelp.exe"), f));
}

#[test]
fn check_removes_only_games_with_nothing_open() {
    let mut list = RunningList::default();
    assert!(list.add("kelp", Path::new(r"C:\Games\Kelp")));
    assert!(!list.add("kelp", Path::new(r"C:\Games\Kelp")));
    list.add("reef", Path::new(r"C:\Games\Reef"));
    let open = vec![PathBuf::from(r"C:\Games\Kelp\Binaries\Win64\kelp.exe"), PathBuf::from(r"C:\Windows\explorer.exe")];
    assert_eq!(list.check(&open), vec!["reef".to_string()]);
    assert_eq!(list.names(), vec!["kelp".to_string()]);
    assert!(list.contains("kelp") && !list.contains("reef"));
    assert_eq!(list.check(&[]), vec!["kelp".to_string()]);
    assert!(list.is_empty());
}

#[test]
fn messages_match_the_spec() {
    assert_eq!(already_running("Kelp Runner"), "Kelp Runner is already running.");
    assert_eq!(close_first("Kelp Runner"), "Close Kelp Runner first.");
}

#[test]
fn open_programs_include_this_test() {
    let me = std::env::current_exe().unwrap();
    let paths = OpenPrograms::new().paths();
    assert!(paths.iter().any(|p| is_inside(p, me.parent().unwrap())));
}
```

- [ ] **Step 2: Run** `cargo test -p fishy-core running` → fails to compile (module missing).
- [ ] **Step 3: Implement.** `is_inside` compares lower-cased, `\`-normalised component lists, so the folder's components must be a prefix of the program's. `OpenPrograms::paths` calls `refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet))` and collects `exe()`. Processes with no path are skipped. Doc comment on the module: paths are only compared in memory, never logged or saved.
- [ ] **Step 4: Run** `cargo test -p fishy-core running` → 4 pass; `cargo clippy -p fishy-core --all-targets -- -D warnings` is clean.
- [ ] **Step 5: Commit** `feat(core): running module (folder match, running list, open programs via sysinfo)`.

### Task 2: Play times and the new setting

**Files:**
- Modify: `crates/fishy-core/src/install/store.rs` (field, `mark_played`, lock, `finish_install` keeps the time)
- Modify: `crates/fishy-core/src/launch/command.rs:131`, `crates/fishy-core/src/launch/mod.rs:115` (add `last_played: None` to test records)
- Modify: `crates/fishy-core/src/settings.rs` (`when_game_starts`, `library_sort` doc mentions `"played"`)

**Interfaces:**
- Produces:
  - `InstalledGame.last_played: Option<DateTime<Utc>>` with `#[serde(default)]`
  - `pub fn mark_played(&self, app: &str, at: DateTime<Utc>) -> io::Result<()>` on `InstallStore`. It does nothing (Ok) when the game isn't installed.
  - `Settings.when_game_starts: String`, default `"keep"`
- A module-level `static INSTALLED_LOCK: std::sync::Mutex<()>` is held around every read-modify-write of `installed.json` (`finish_install`, `forget_installed`, `mark_played`).

- [ ] **Step 1: Write failing tests:**
  - `store.rs`: `play_time_is_saved_and_kept_by_an_update`: install the sample job, `mark_played("Tide", t)`, `installed()[0].last_played == Some(t)`; run `finish_install` again (update) → still `Some(t)` and `installed_at` unchanged; `mark_played("Nope", t)` is Ok and changes nothing.
  - `store.rs`: `old_installed_json_without_play_time_loads`: write a JSON record without `last_played` → loads with `None`.
  - `store.rs`: `install_and_play_time_writes_both_survive`: two games; `mark_played` on one, then `finish_install` of the other → the first still has its time.
  - `settings.rs`: `when_game_starts_defaults_to_keep_and_round_trips`: default `"keep"`; an old file without it → `"keep"`; saving `"minimize"` loads back `"minimize"`.
- [ ] **Step 2: Run** `cargo test -p fishy-core` → new tests fail.
- [ ] **Step 3: Implement** as in Interfaces (the lock guard uses `unwrap_or_else(|e| e.into_inner())`, as elsewhere).
- [ ] **Step 4: Run** `cargo test -p fishy-core` → all pass; clippy is clean.
- [ ] **Step 5: Commit** `feat(core): save play times in installed.json; when_game_starts setting`.

### Task 3: App: watching, Play, Update, Uninstall, minimize

**Files:**
- Create: `app/src-tauri/src/running.rs`
- Modify: `app/src-tauri/src/lib.rs` (`mod running;`, `app.manage(running::AppRunning::default())`, register `running::running_list`, the Uninstall refusal)
- Modify: `app/src-tauri/src/play.rs` (refuse a running game, save the play time, start watching, minimize)
- Modify: `app/src-tauri/src/updates.rs` (`queue_update` refuses a running game)

**Interfaces:**
- Consumes: `RunningList`, `OpenPrograms`, `already_running`, `close_first` (Task 1); `InstallStore::mark_played`, `Settings.when_game_starts` (Task 2).
- Produces:
  - `pub struct AppRunning` (Default) holding `Mutex<Watch>` where `struct Watch { list: RunningList, watching: bool, minimized_by_us: bool }` (std Mutex; never held across `.await`)
  - `impl AppRunning { pub fn contains(&self, app: &str) -> bool }`
  - `pub fn started(app: &AppHandle, app_name: &str, folder: &Path, minimize: bool)`: adds the game, logs `{app_name} is running`, emits `running-changed` (a `Vec<String>`), minimizes the main window when `minimize` (setting `minimized_by_us`), and spawns the loop if `watching` is false
  - the loop: `sleep(5 s)`; `OpenPrograms::paths` in `spawn_blocking` (one `OpenPrograms` owned by the loop); `check`; log `{app} closed` for each; emit when anything changed; when empty, set `watching = false` under the same lock, restore if `should_restore`, and end
  - `fn should_restore(now_empty: bool, minimized_by_us: bool) -> bool`
  - `fn restore(app: &AppHandle)`: main window `unminimize()` then `set_focus()`, errors ignored
  - `#[tauri::command] pub fn running_list(running: State<AppRunning>) -> Vec<String>`

- [ ] **Step 1: Write failing tests** in `running.rs`:
  - `restore_only_when_empty_and_we_minimized`: `should_restore(true, true)` only; `(false, true)`, `(true, false)` false.
  - `contains_follows_the_list`: an `AppRunning` with `kelp` added through the inner list → `contains("kelp")`, not `contains("reef")`.
- [ ] **Step 2: Run** `cargo test -p fishy-client running` → fails.
- [ ] **Step 3: Implement `running.rs`.**
- [ ] **Step 4: Wire Play.** In `launch_game`, right after finding `installed`, return `PlayError { message: already_running(&title), can_offline: false, signed_out: false }` when `contains`. After `start` succeeds: `store.mark_played(&app_name, Utc::now())` (on Err log `couldn't save play time for {app_name}`), then `running::started(&app, &app_name, &installed.install_path, settings.when_game_starts == "minimize")`. `play_game` gains `running` and `settings` states and passes `app` down.
- [ ] **Step 5: Wire Update and Uninstall.** `queue_update`: after finding `installed`, `Err(close_first(&installed.title))` when running. `uninstall_game`: before `take_stopped_update`, the same check (title from `store.installed()`, else `app_name`).
- [ ] **Step 6: Run** `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` → all pass, clean.
- [ ] **Step 7: Commit** `feat(app): watch games started by Play, refuse Play/Update/Uninstall while running, minimize option`.

### Task 4: UI state: running store, Recently played, auto-update after closing

**Files:**
- Modify: `app/src/lib/install/installs.ts` (`InstalledGame.last_played?: string | null`, `running` store, listener, `autoUpdate` skips running)
- Modify: `app/src/lib/library/library.ts` (`sortGames` `"played"`, update after a game closes)
- Modify: `app/src/lib/settings/settings.ts` (`LibrarySort` adds `"played"`; `WhenGameStarts = "keep" | "minimize"`; `when_game_starts` default `"keep"`)
- Test: `app/src/lib/library/library.test.ts`, `app/src/lib/install/installs.test.ts`

**Interfaces:**
- Produces:
  - `export const running = writable<string[]>([])` (reset by `resetInstalls`)
  - `initInstalls` listens to `running-changed` (payload `string[]`, anything else → `[]`) and loads `running_list` once
  - `sortGames(…, "played", …)`: played installed games by `last_played`, newest first; then installed games without a time, A–Z; then the rest A–Z
  - library.ts subscribes to `running`. When names disappear and `get(settings).auto_update` is on, it calls `autoUpdate(current.latestVersions)`.

- [ ] **Step 1: Write failing tests:**
  - `sorts_recently_played_then_other_installed_then_the_rest`: games Abalone (installed, played 2026-11-01), Barnacle (not installed), coral (installed, never played), Dory (installed, played 2026-11-20) → `Dory, Abalone, coral, Barnacle`; the input array is unchanged.
  - `running_follows_the_backend_event`: mock `listen` so `running-changed` delivers `["kelp"]` → `get(running)` equals `["kelp"]`; `running_list` result used at start.
  - `auto_update_skips_running_games_and_queues_them_when_closed`: auto_update on, kelp outdated and running → no `update_start` after a refresh; set `running` to `[]` → `update_start` called with `kelp`.
- [ ] **Step 2: Run** `npm.cmd run test` from `app/` → new tests fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `npm.cmd run test` and `npm.cmd run check` → all pass, 0 errors.
- [ ] **Step 5: Commit** `feat(ui): running store, Recently played sort, auto-update after a game closes`.

### Task 5: UI screens: card, Sort menu, Settings, dev mock

**Files:**
- Modify: `app/src/lib/library/LibraryPage.svelte` (Running button, disabled Update/Uninstall, note, Sort option)
- Modify: `app/src/lib/settings/SettingsPage.svelte` ("When a game starts" select in "Appearance and startup")
- Modify: `app/src/lib/auth/devMock.ts` (`running_list` → `[]`, `when_game_starts`/`library_sort` in `devSettings`)
- Test: `app/src/lib/library/library.test.ts`, `app/src/lib/settings/SettingsControls.test.ts`

**Interfaces:**
- Consumes: `running` (Task 4), `when_game_starts` (Task 4 settings type).
- The card: `starting` wins (Starting…). Otherwise, when running: `<button class="play" aria-label="{title} is running" disabled>Running</button>`, Update button `disabled`, the "⋯" Uninstall item `disabled`, and `<p class="note small">Close {title} first.</p>`.

- [ ] **Step 1: Write failing tests:**
  - `a_running_game_shows_running_and_greys_out_its_buttons`: installed + outdated kelp, `running.set(["kelp"])` → button "Kelp Runner is running" disabled with text "Running"; "Update Kelp Runner" disabled; Uninstall menu item disabled; text "Close Kelp Runner first." shown; `running.set([])` → "Play Kelp Runner" enabled and the note gone.
  - `sort_menu_offers_recently_played`: choosing it saves `library_sort: "played"`.
  - `when_a_game_starts_saves_and_shows_the_saved_value_after_a_failed_save`: the select labelled "When a game starts" has both option texts; choosing minimize calls `settings_set` with `when_game_starts: "minimize"`; when `settings_set` rejects, the select shows "keep" again and "Couldn't save your settings." appears.
- [ ] **Step 2: Run** `npm.cmd run test` → new tests fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `npm.cmd run test` and `npm.cmd run check` → all pass, 0 errors.
- [ ] **Step 5: Commit** `feat(ui): Running on the card, Recently played sort, When a game starts setting`.

### Task 6: Finish

- [ ] **Step 1:** Run everything: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `npm.cmd run test`, `npm.cmd run check`. Note the counts.
- [ ] **Step 2:** Fresh whole-branch review of this sub-project's commits (most capable model) against §6f.4 and Review Focus. Fix what it confirms; commit.
- [ ] **Step 3:** Update spec §12: what's done (Polish 4), test counts, a "Still to check by hand (milestone 7, sub-project 4)" list, the privacy decision (program paths compared in memory only), and Next steps → sub-project 5. Commit `docs: §12 for Polish 4 (Running games)` and `git push`.
- [ ] **Step 4:** Hand check with the user in `npm.cmd run tauri dev`: Play a game → Running and greyed-out buttons; close it → Play comes back within about 5 s; Minimize setting → window minimizes and comes back; Recently played order. Record the result in §12, commit, push.
