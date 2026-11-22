# Fishy Client updates itself Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** From 1.1.0 on, Fishy Client finds newer versions on GitHub Releases at start-up and, after the user presses Update now, installs a signed update and restarts.

**Architecture:** `tauri-plugin-updater` (v2) checks `latest.json` on the public repository's newest Release; a small app module `app/src-tauri/src/app_update.rs` exposes `app_update_check` and `app_update_install` commands and emits progress; a Svelte bar in the app shell shows the offer. Releases are signed with a password-less key kept outside the repo; `scripts/make-latest-json.mjs` writes `latest.json`.

**Tech Stack:** Rust (tauri-plugin-updater 2, tauri-plugin-process 2, semver via the plugin), Svelte 5, vitest, Node script.

**Spec:** `docs/superpowers/specs/2026-11-20-fishy-client-design.md` §6j (also §6i notes, §12 privacy).

## Global Constraints

- Endpoint exactly `https://github.com/Gnammahh/fishy-client/releases/latest/download/latest.json`; `plugins.updater.windows.installMode` `"passive"`; `bundle.createUpdaterArtifacts` `true`.
- Signing key: `%USERPROFILE%\.fishy-client-keys\fishy-client.key`, no password, never in the repo, never printed. Builds get `TAURI_SIGNING_PRIVATE_KEY` = that **path** and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` = empty.
- Setting `check_app_updates` (Rust `bool`, TS `boolean`), default `true`; switch name `Check for Fishy Client updates`, hint `Fishy Client asks GitHub at start-up whether a newer version exists. Only a normal download request is sent.`
- Copy: bar `Fishy Client {v} is available.`, buttons `What's new`, `Update now`, `Later`; progress `Downloading the update… {n}%`; error `Couldn't download the update. Try again later.`; running game `Close {title} first.` (reuse `fishy_core::running::close_first`).
- Logs: `app update: {v} available`, `app update: installing {v}`, `app update: check failed` only.
- PowerShell: `npm.cmd`. Commits end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Never push `master`, never make the repo public or create a Release without the user's go.

## Review Focus

1. Offered version equal to or older than the running one: no bar (Task 2 test `newer_than_compares_versions`, including `1.10.0 > 1.9.0` and `1.1.0` vs `1.1.0`).
2. A game running when Update now is pressed: refused with "Close *Title* first.", nothing downloaded (Task 2 test `install_refused_while_a_game_runs` on the pure guard `install_blocked`).
3. A download in progress when updating: paused before the installer runs (Task 2: `app_update_install` calls `downloads.pause_all` before `download_and_install`; test on the order via the pure helper `install_steps`).
4. Setting off: no network request at all (Task 2 test `no_check_when_switched_off` on `should_check`).
5. `latest.json` built from the wrong `.sig` or a version that doesn't match the notes: the script refuses (Task 3 test `refuses_mismatched_version`).

---

### Task 1: The setting and the plugin wiring

**Files:**
- Modify: `crates/fishy-core/src/settings.rs`, `app/src/lib/settings/settings.ts`, `app/src/lib/settings/SettingsPage.svelte` (+ tests: `SettingsPage.test.ts` default object gains `check_app_updates: true`)
- Modify: `app/src-tauri/Cargo.toml` (`tauri-plugin-updater = "2"`, `tauri-plugin-process = "2"`), `app/src-tauri/src/lib.rs` (`.plugin(tauri_plugin_updater::Builder::new().build())`, `.plugin(tauri_plugin_process::init())`; bundle test), `app/src-tauri/tauri.conf.json` (`bundle.createUpdaterArtifacts`, `plugins.updater` with `endpoints`, `windows.installMode`, `pubkey` placeholder `"REPLACE_WITH_PUBLIC_KEY"` until Task 4), `app/src-tauri/capabilities/default.json` (no new JS permissions: the updater is only used from Rust)

**Interfaces:**
- Produces: `Settings.check_app_updates` (default true).

- [ ] **Step 1: Failing tests**: Rust `check_app_updates_defaults_to_on` (default + old file); bundle test also asserts `bundle["createUpdaterArtifacts"] == true`, `plugins.updater.endpoints == [endpoint]`, `plugins.updater.windows.installMode == "passive"`, `plugins.updater.pubkey` is a non-empty string; UI `app_update_switch_is_on_and_saves`.
- [ ] **Step 2:** `cargo test --workspace` / `npm.cmd test -- SettingsPage` → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `cargo test --workspace`, clippy, `npm.cmd test`, `npm.cmd run check` → PASS.
- [ ] **Step 5:** Commit `feat(app-update): setting and updater plugin`.

### Task 2: Check and install commands

**Files:**
- Create: `app/src-tauri/src/app_update.rs` (register module + both commands in `lib.rs`)

**Interfaces:**
- Consumes: `Settings.check_app_updates`; `AppInstall.downloads.pause_all`; `running::AppRunning` (`in_use()` / running titles); `tauri_plugin_updater::UpdaterExt`.
- Produces:
  - pure: `fn newer_than(current: &str, offered: &str) -> bool` (semver, a bad version = false), `fn should_check(s: &Settings) -> bool`, `fn install_blocked(running_titles: &[String]) -> Option<String>` (first title → `close_first(title)`), `fn install_steps() -> [&'static str; 3]` = `["pause downloads", "download and install", "restart"]` (documents and pins the order the command follows)
  - `#[tauri::command] async fn app_update_check(app) -> Option<AppUpdate>` where `AppUpdate { version: String, notes: String }` (None: switched off, nothing newer, or check failed — logged)
  - `#[tauri::command] async fn app_update_install(app) -> Result<(), String>`: refuse if a game runs; `pause_all`; `check()` again, `download_and_install` emitting `app-update-progress` `{ percent: u8 }`; then `app.restart()`. Errors → the spec's error copy.

- [ ] **Step 1: Failing tests**: `newer_than_compares_versions`, `no_check_when_switched_off`, `install_refused_while_a_game_runs`, `install_pauses_downloads_first` (asserts `install_steps()[0] == "pause downloads"`).
- [ ] **Step 2:** `cargo test -p fishy-client app_update` → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `cargo test --workspace`, clippy → PASS.
- [ ] **Step 5:** Commit `feat(app-update): check and install commands`.

### Task 3: The update bar and the latest.json script

**Files:**
- Create: `app/src/lib/appupdate/appUpdate.ts` (store + `checkForUpdate()`, `installUpdate()`, progress listener), `app/src/lib/appupdate/UpdateBar.svelte`, `app/src/lib/appupdate/UpdateBar.test.ts`
- Modify: `app/src/lib/shell/AppShell.svelte` (call `checkForUpdate()` once on mount; render `<UpdateBar />` at the top of `main`)
- Create: `scripts/make-latest-json.mjs` (exports `makeLatestJson({ version, notes, signature, url, pubDate })`; its command-line part reads the version from `app/package.json`, the top entry of `app/src/lib/notes/release-notes.json`, and the `.sig` next to the built `.msi`, then writes `latest.json` beside it), `scripts/make-latest-json.test.mjs`
- Create: `app/src/lib/notes/release-notes.json` (the notes data, moved out of `releaseNotes.ts`, which now imports it: one source for the app and the script)

**Interfaces:**
- Consumes: Task 2 commands and event; `RELEASE_NOTES`.
- Produces: `latest.json` shape from spec §6j.

- [ ] **Step 1: Failing tests**
  - UI `bar_shows_version_and_buttons`, `whats_new_shows_the_notes`, `later_hides_it`, `update_now_installs_and_shows_progress` (emits a progress event → `Downloading the update… 42%`), `update_error_is_shown`, `close_game_first_is_shown`, `no_bar_when_nothing_newer`.
  - Script `writes_tauri_static_json` (exact object with `platforms["windows-x86_64"]`), `refuses_mismatched_version` (notes entry ≠ package version → throws).
- [ ] **Step 2:** `npm.cmd test -- UpdateBar make-latest-json` → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `npm.cmd test`, `npm.cmd run check` → PASS.
- [ ] **Step 5:** Commit `feat(app-update): update bar and latest.json script`.

### Task 4: Key, docs, review, release

- [ ] Generate the key (with the user's OK, it creates an access key): `npm.cmd run tauri signer generate -- --ci -w %USERPROFILE%\.fishy-client-keys\fishy-client.key`; put the **public** key (`.key.pub` contents) into `plugins.updater.pubkey`; tell the user to back up the folder.
- [ ] `docs/RELEASING.md` (the 4 release steps from §6j, with exact PowerShell commands); README: privacy table gains github.com, the "doesn't update itself" line replaced, Settings switch mentioned; spec §12 entry, test counts, hand checks.
- [ ] Fresh whole-branch review (most capable model); fix Critical/Important test-first.
- [ ] Push `feat/import`. Then, with the user: force-push the cleaned `master`/`feat/polish`, make the repo public, merge `feat/import` into `master`, build signed 1.1.0, write `latest.json`, create Release v1.1.0; later 1.1.1 to prove the update arrives.
