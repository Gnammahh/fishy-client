# Show games in Epic's launcher Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Games installed or imported in Fishy Client appear as installed in Epic's launcher, kept in step by a setting that is on by default.

**Architecture:** A core module `fishy_core::epic_sync` writes/removes one Epic `.item` plus `.egstore\<GUID>.manifest` and `.mancpn` per game (Legendary's `egl_export` format), updating an existing Epic entry for the same folder in place. A small app module `app/src-tauri/src/epic_sync.rs` calls it at start-up, when the switch turns on, after an install/update/import finishes, and after an uninstall. `Settings.show_in_epic` (default true) plus one switch on the Settings page.

**Tech Stack:** Rust (serde_json, uuid v4, tempfile), Tauri 2, Svelte 5, vitest.

**Spec:** `docs/superpowers/specs/2026-11-20-fishy-client-design.md` §6h (and §6g, §6c, §12).

## Global Constraints

- Epic folder: `%PROGRAMDATA%\Epic\EpicGamesLauncher\Data\Manifests` (reuse `fishy_core::epic_import::epic_manifests_dir()`). If it doesn't exist, write nothing and never create it.
- Only while `show_in_epic` is true. Switch off: nothing written or removed.
- JSON: 4-space indent, sorted keys (serde_json's default `Map` is sorted), UTF-8 without BOM. Write next to the target, then rename.
- New GUID: 32 uppercase hex, `uuid::Uuid::new_v4().simple()` uppercased.
- Never touch `LauncherInstalled.dat` or any other Epic file.
- A failure never fails the install/update/import/uninstall. Log line exactly `epic sync: couldn't update Epic's list for {n} game(s)` style: `"epic sync: couldn't update Epic's list for 1 game"` / `"… for 3 games"`, plus app names only, never paths.
- Switch copy: name `Show my games in Epic's launcher`, hint `Epic's launcher sees games installed with Fishy Client. Changes show the next time Epic's launcher starts.`
- PowerShell: `npm.cmd`. Commits end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. Epic has two entries for the same app, one in this folder and one elsewhere: only the same-folder one is updated (Task 2 test `updates_only_the_same_folder_entry`).
2. Epic wrote the folder as `C:/Program Files/Epic Games/Sugar` and the record says `C:\Program Files\Epic Games\sugar\`: still the same folder (Task 2 test `same_folder_ignores_case_slashes_and_trailing_slash`).
3. Updating Epic's own entry keeps the fields Fishy Client doesn't own (`InstallComponents`, `ProcessNames`, `InstallSessionId`…) (Task 2 test `in_place_update_keeps_epics_other_fields`).
4. Uninstall when Epic's entry is for another folder: Epic's entry is not deleted (Task 2 test `remove_leaves_another_folders_entry`).
5. Epic's folder missing: nothing is created, for write and remove (Task 2 test `no_epic_folder_writes_nothing`).

---

### Task 1: The `show_in_epic` setting and its switch

**Files:**
- Modify: `crates/fishy-core/src/settings.rs` (field + default + tests)
- Modify: `app/src/lib/settings/settings.ts` (type + default), `app/src/lib/settings/SettingsPage.svelte` (switch after "Warn before playing…", `toggle` key union gains `"show_in_epic"`, error shown in the downloads area)
- Test: `crates/fishy-core/src/settings.rs`, `app/src/lib/settings/SettingsPage.test.ts`

**Interfaces:**
- Produces: `Settings.show_in_epic: bool` (Rust, default `true`), `show_in_epic: boolean` (TS, default `true`).

- [ ] **Step 1: Failing tests**
  - Rust `show_in_epic_defaults_to_on`: `Settings::default().show_in_epic` and an old JSON `{"auto_update":false}` both read `true`; a saved `false` round-trips.
  - UI `show_in_epic_switch_is_on_and_saves`: the switch named "Show my games in Epic's launcher" is checked by default; clicking it calls `settings_set` with `value.show_in_epic === false`.
- [ ] **Step 2:** `cargo test -p fishy-core show_in_epic` and `npm.cmd test -- SettingsPage` (from `app/`) → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `cargo test --workspace`, `npm.cmd test`, `npm.cmd run check` → PASS.
- [ ] **Step 5:** Commit `feat(epic-sync): setting to show games in Epic's launcher`.

### Task 2: Writing and removing Epic entries (core)

**Files:**
- Create: `crates/fishy-core/src/epic_sync.rs` (register in `crates/fishy-core/src/lib.rs`)
- Modify: `crates/fishy-core/Cargo.toml` (`uuid = { version = "1", features = ["v4"] }`, already in Cargo.lock through Tauri)
- Modify: `crates/fishy-core/src/install/store.rs` (`write_atomic` becomes `pub(crate)`)

**Interfaces:**
- Consumes: `InstalledGame` (store.rs), `epic_import::epic_manifests_dir`.
- Produces:
  - `pub struct SyncInfo { pub namespace: String, pub catalog_item_id: String, pub can_run_offline: bool, pub requires_ot: bool }`
  - `pub enum Synced { Written { guid: String }, OtherCopy, NoEpic }` (Debug, PartialEq)
  - `pub fn new_guid() -> String`
  - `pub fn write_entry(epic_dir: &Path, game: &InstalledGame, info: &SyncInfo, manifest: &[u8], guid: impl FnOnce() -> String) -> io::Result<Synced>`
  - `pub fn remove_entry(epic_dir: &Path, game: &InstalledGame) -> io::Result<bool>` (true if something was removed)

Rules (from spec §6h):
- Find entries by reading every `*.item` as a `serde_json::Value` (BOM stripped, unreadable skipped) and comparing `AppName`. Same folder = `InstallLocation` and `game.install_path` equal after lowercasing, `/`→`\`, trimming trailing `\`.
- Same folder found: start from that entry's JSON object (keep unknown fields), keep its `InstallationGuid`, overwrite the fields below, write it back to the same file path.
- Same app in another folder only: return `OtherCopy`, write nothing.
- None: start from the template, `guid()` for `InstallationGuid`, file `<epic_dir>\<GUID>.item`.
- Fields always set: the record fields (`AppName`, `DisplayName`, `AppVersionString`, `InstallLocation` as `install_path.display()`, `InstallSize`, `LaunchCommand`, `LaunchExecutable`), the `SyncInfo` fields (`CatalogNamespace`, `CatalogItemId`, `bCanRunOffline`, `OwnershipToken` as `"true"`/`"false"`), `MainGameAppName` = AppName, `ManifestLocation` = `<folder>/.egstore`, `StagingLocation` = `<folder>/.egstore/bps`, `bIsIncompleteInstall` false, `bNeedsValidation` false. Template-only (new entries) adds every other spec §6h field with its listed value.
- Then write `<folder>\.egstore\<GUID>.manifest` (the bytes) and `<GUID>.mancpn` (`AppName`, `CatalogItemId`, `CatalogNamespace`, `FormatVersion` 0). Create `.egstore` if needed (inside the game folder only).
- `remove_entry`: same-folder entry only → delete its `.item`, `.egstore\<GUID>.manifest`, `.egstore\<GUID>.mancpn`, then `.egstore` if empty, then the game folder if empty. Missing files are fine.
- `epic_dir` missing → `Ok(Synced::NoEpic)` / `Ok(false)`, nothing created.

- [ ] **Step 1: Failing tests** (tempfile; a fake `Manifests` dir and game folder; a helper record and `SyncInfo`):
  - `writes_a_new_entry`: `Written{guid:"ABC…"}` with the injected guid; the `.item` parses with every spec field (spot-check `AppVersionString`, `InstallLocation`, `LaunchExecutable`, `OwnershipToken == "false"`, `bIsIncompleteInstall == false`, `AppCategories == ["public","games","applications"]`, `FormatVersion == 0`); `.egstore\<guid>.manifest` equals the bytes; `.mancpn` has the 4 keys; file text starts with `{\n    "` (4 spaces) and has no BOM.
  - `new_guid_is_32_uppercase_hex`
  - `updates_only_the_same_folder_entry`: Epic entries for the app in this folder (guid `E1`, version `old`) and in `D:\Other` (guid `E2`) → `Written{guid:"E1"}`, `E1.item` has the new version, `E2.item` byte-for-byte unchanged, no third `.item`.
  - `same_folder_ignores_case_slashes_and_trailing_slash`
  - `in_place_update_keeps_epics_other_fields`: existing entry with `"InstallComponents": ["X"]`, `"InstallSessionId": "S"` keeps both.
  - `another_folder_only_is_left_alone`: → `OtherCopy`, no files written.
  - `no_epic_folder_writes_nothing`: missing dir → `NoEpic` and `Ok(false)` from remove, dir still missing, game folder has no `.egstore`.
  - `remove_deletes_only_its_files`: after a write, an extra Epic file `.egstore\OTHER.manifest` stays; the `.item`, `.manifest`, `.mancpn` are gone.
  - `remove_cleans_empty_folders`: write then remove with no other files and an otherwise empty game folder → `.egstore` and the game folder are gone.
  - `remove_leaves_another_folders_entry`: Epic entry for another folder only → `Ok(false)`, entry still there.
- [ ] **Step 2:** `cargo test -p fishy-core epic_sync` → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `cargo test -p fishy-core --lib`, `cargo clippy --workspace --all-targets -- -D warnings` → PASS/clean.
- [ ] **Step 5:** Commit `feat(epic-sync): write and remove Epic launcher entries`.

### Task 3: When the app syncs

**Files:**
- Create: `app/src-tauri/src/epic_sync.rs` (register `mod epic_sync;` in lib.rs)
- Modify: `app/src-tauri/src/lib.rs` (start-up: after `app.manage(settings)`, spawn `epic_sync::sync_all`; `uninstall_game`: take the record before uninstalling, call `epic_sync::remove_one` after success), `app/src-tauri/src/downloads.rs` (`finish` Ok branch: spawn `epic_sync::sync_one`), `app/src-tauri/src/epic_import.rs` (`Added`: `sync_one`), `app/src-tauri/src/updates.rs` (`settings_set`: if `turned_on(&old, &value)` spawn `sync_all`)

**Interfaces:**
- Consumes: Task 2 functions; `Settings.show_in_epic`; `LibraryCache::load()`; `InstallStore::{installed, installed_manifest}`.
- Produces:
  - `pub(crate) fn turned_on(old: &Settings, new: &Settings) -> bool`
  - `pub(crate) fn sync_info(game: &Game) -> SyncInfo` (from `Game.namespace`, `catalog_item_id`, `launch.can_run_offline`, `launch.requires_ot`)
  - `pub(crate) fn failed_line(apps: &[String]) -> String` → `"epic sync: couldn't update Epic's list for 1 game (A)"` / `"… for 2 games (A, B)"`
  - `pub(crate) async fn sync_one(app: &tauri::AppHandle, app_name: &str)`, `pub(crate) async fn sync_all(app: &tauri::AppHandle)`, `pub(crate) async fn remove_one(app: &tauri::AppHandle, game: InstalledGame)`: each returns at once when the switch is off or Epic's folder is missing; file work in `spawn_blocking`; a game missing from the saved Library or without a saved manifest is skipped (counted as not updated); log `info!("epic sync: {n} games in Epic's list")` after `sync_all`.

- [ ] **Step 1: Failing tests** in `epic_sync.rs`: `turned_on_only_from_off_to_on`, `sync_info_from_library_game` (requires_ot true → `requires_ot`, can_run_offline false kept), `failed_line_counts_and_names`.
- [ ] **Step 2:** `cargo test -p fishy-client epic_sync` → FAIL.
- [ ] **Step 3:** Implement and wire the five call sites.
- [ ] **Step 4:** `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings` → PASS/clean.
- [ ] **Step 5:** Commit `feat(epic-sync): keep Epic's list in step on start, install, update, import and uninstall`.

### Task 4: Docs, review, push, hand check

- [ ] README: "What is stored on your PC" gains a row: `C:\ProgramData\Epic\EpicGamesLauncher\Data\Manifests\` and each game's `.egstore\` — entries so Epic's launcher sees your games (only while **Show my games in Epic's launcher** is on; off by Settings). Update the "Games from Epic's launcher" section and the "Your files" sentence (Fishy Client now writes these small list files when the switch is on).
- [ ] Spec §12: progress-log entry, test counts, privacy line, hand-check list: with Epic closed, start Fishy Client → open Epic's launcher → Bendy shows as installed and doesn't offer to download; Launch from Epic works; turn the switch off, uninstall a test game in Fishy Client → its Epic entry stays (by design); turn on again.
- [ ] Fresh whole-branch review (most capable model), fix Critical/Important test-first.
- [ ] `git push` (branch `feat/import`).
- [ ] Restart `npm.cmd run tauri dev` in `app/` for the hand check.
