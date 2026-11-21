# Polish 1: Settings, Logs and Your Data Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give Fishy Client log files with secrets filtered out, an Account section in Settings (Log out, Open logs folder, Delete all my data), and four new settings: default install folder, download speed limit, Reduce motion, Start with Windows.

**Architecture:**
- **fishy-core** gains these pieces:
  - a `logs` module: a scrubber, a daily file writer that sends every line through the scrubber, and pruning of old files
  - a `wipe` module that empties `%APPDATA%\FishyClient`
  - a `SpeedLimiter` (token bucket) used by the install engine before each chunk download
  - new fields in `Settings`
- **The app** (`app/src-tauri`) gains:
  - logging set up at startup, plus `info!`/`warn!` lines at the key steps
  - the commands `open_logs_folder`, `delete_all_data`, `autostart_get` and `autostart_set`
  - one shared limiter, updated by `settings_set`
- **The UI** gains:
  - new sections on the Settings page and a Delete box
  - Log out removed from the sidebar
  - a `reducedMotion` store that also follows the setting

**Tech Stack:** Rust (`tracing`, `tracing-subscriber`, `tracing-appender`, `tauri-plugin-autostart`, tempfile, tokio test-util), Tauri 2, Svelte 5, vitest.

**Spec:** `docs/superpowers/specs/2026-11-20-fishy-client-design.md` §6f.1 (and §12 "Privacy and security decisions")

## Global Constraints
- **Log folder:** `%APPDATA%\FishyClient\logs\`. Files are named `fishy-client.YYYY-MM-DD.log`, a new one each day, logged at Info level and up. Files older than 7 days are deleted at startup.
- **Never logged:** tokens, sign-in or exchange codes, Epic's raw error text, the Epic account ID, the email or display name. Log the user-facing message instead of the internal error.
- **The scrubber replaces with `[redacted]`:**
  - whatever follows `Authorization:`, `bearer ` or `basic ` (any letter case)
  - the values of `code`, `token`, `access_token`, `refresh_token`, `exchange_code` and `password`, written as `key=value` or `"key": "value"`
  - `eg1~…` tokens
  - any unbroken run of 32 or more of `A-Za-z0-9-_+/=`, except inside a Windows path
- **UI text exactly as in §6f.1:**
  - "Delete all my data…"
  - "This logs you out and removes everything Fishy Client saved on this PC: settings, your game list, download progress and logs."
  - "Also delete my installed games (N games, X GB)"
  - "Your game folders stay on disk, but Fishy Client will forget them."
  - "Some files couldn't be deleted (N). Close any running games and try again."
  - "Your default folder isn't available, so this uses <default>."
  - "Pick a number from 1 to 1000."
  - "Couldn't save your settings."
  - "Couldn't open the folder picker."
  - "Couldn't change Start with Windows."
  - "Couldn't open the logs folder. It's at %APPDATA%\FishyClient\logs."
  - Section titles: "Downloads", "Appearance and startup", "Account"
  - Switch names: "Default install folder", "Limit download speed", "Reduce motion", "Start with Windows"
  - Buttons: "Change…", "Use default", "Open logs folder", "Log out", "Cancel", "Delete"
- **`settings.json` defaults:** `auto_update: false`, `warn_outdated_play: true`, `install_folder: null`, `speed_limit_mbps: null`, `reduce_motion: false`. Every new field is `#[serde(default)]`. The speed limit is a whole number from 1 to 1000 MB/s, where 1 MB = 1,000,000 bytes.
- **Start with Windows** is never saved in `settings.json`; the switch always reads the Registry state.
- **Open logs folder** runs `explorer.exe <logs dir>` from Rust. Don't add an opener or shell plugin.
- PowerShell blocks plain `npm`, so run UI commands as `npm.cmd` from `app/`.

## Review Focus
- **The speed box gets "", "2.5", "abc", "0" or "1001":** each one is refused with "Pick a number from 1 to 1000." and nothing is saved. Tested in Task 10.
- **Long Windows paths and game names in a log line** (for example `C:\Users\you\Games\SomeVeryLongGameFolderNameWithoutSpaces1234`) stay readable. Only secret-shaped text outside paths is masked. Tested in Task 1.
- **Delete all my data while a download is running:** the download stops first. With the tick box on, its half-written game folder is discarded too. With it off, that folder stays, like an installed game. Tested in Task 5.
- **Delete pressed twice, or Cancel pressed while deleting:** both buttons are disabled while the delete runs, and the command is called once. Tested in Task 6.
- **The speed limit after a restart:** the limiter starts from the saved setting, not "no limit". Tested in Task 8 (`limit_bytes`).

---

## Half 1a: logs, Account, Delete all my data

### Task 1: The scrubber

**Files:**
- Create: `crates/fishy-core/src/logs/mod.rs` (`pub mod scrub;` for now), `crates/fishy-core/src/logs/scrub.rs`
- Modify: `crates/fishy-core/src/lib.rs` (`pub mod logs;`)

**Interfaces:**
- Produces: `pub fn scrub(line: &str) -> std::borrow::Cow<'_, str>`, which hands back the same text when nothing matched

- [ ] **Step 1: Write the failing tests** in `scrub.rs`:
  - `masks_authorization_values`: `"Authorization: bearer abc.def"` gives `"Authorization: [redacted]"`, and `"sent bearer eg1~xyz"` gives `"sent bearer [redacted]"`.
  - `masks_key_values`: `"code=ABC123&x=1"` gives `"code=[redacted]&x=1"`, and `r#"{"refresh_token": "r1"}"#` gives `r#"{"refresh_token": "[redacted]"}"#`. Cover each of the six keys.
  - `masks_eg1_tokens`: `"got eg1~QWERTY.zxc-1"` gives `"got [redacted]"`.
  - `masks_long_random_runs`: a 40-character hex string inside text is masked, and a 31-character one is not.
  - `keeps_normal_lines_and_paths`: these come back unchanged:
    - `"installed Bad Cheese (1.2 GB) in 34 s"`
    - `r"C:\Users\you\Games\SomeVeryLongGameFolderNameWithoutSpaces1234\game.exe"`
    - `"library refresh took 5.8 s (164 lookups)"`
- [ ] **Step 2: Run them and see them fail.** Run `cargo test -p fishy-core logs::scrub`. Expected: FAIL (not defined).
- [ ] **Step 3: Implement `scrub`.** Add the `regex` crate to fishy-core if it isn't already a dependency. Apply the patterns in the order listed in Global Constraints. For the long-run rule, skip any match that touches a `\` on either side, or that follows a `X:\` drive prefix in the same whitespace-separated word.
- [ ] **Step 4: Run the tests again.** Same command. Expected: PASS.
- [ ] **Step 5: Commit.** `feat(core): log scrubber masks secret-shaped text`

### Task 2: Log files

**Files:**
- Modify: `crates/fishy-core/src/logs/mod.rs`, `crates/fishy-core/Cargo.toml` (add `tracing`, `tracing-subscriber` with the `fmt` feature, and `tracing-appender`)

**Interfaces:**
- Consumes: `scrub`
- Produces:
  - `pub fn default_dir() -> PathBuf`, which is `%APPDATA%\FishyClient\logs`
  - `pub fn prune(dir: &Path, today: chrono::NaiveDate, keep_days: u32) -> usize`, which deletes `fishy-client.YYYY-MM-DD.log` files older than `keep_days` and returns how many it removed; other files are left alone
  - `pub struct ScrubWriter<W: Write>(pub W)`, whose `write` passes the buffer through `scrub` before writing it on
  - `pub fn init(dir: &Path) -> std::io::Result<LogGuard>`, which creates the folder, prunes with 7 days, and installs a global subscriber at Info level over a daily appender wrapped in `ScrubWriter` (prefix `fishy-client`, suffix `log`); `LogGuard` holds the appender's `WorkerGuard`
  - `pub fn make_writer(dir: &Path) -> impl for<'a> MakeWriter<'a>`, the same writer without the global install, for tests

- [ ] **Step 1: Write the failing tests:**
  - `prune_removes_only_old_log_files`:
    - Set up a temp folder with logs dated 2026-11-01, 2026-11-14, 2026-11-15 and 2026-11-21, plus `notes.txt`.
    - Call `prune` with today 2026-11-21 and 7 days.
    - The result is 2; the 11-15 and 11-21 logs stay, and so does `notes.txt`.
  - `a_logged_token_is_written_redacted`:
    - Build a subscriber with `make_writer(tmp)`.
    - Inside `tracing::subscriber::with_default`, log `info!("refreshing with access_token=SECRET123")`.
    - Read the log file: it contains `access_token=[redacted]` and not `SECRET123`.
  - `init_fails_cleanly_when_the_folder_cant_be_made`: `init` on a path under a *file* returns `Err`.
- [ ] **Step 2: Run them and see them fail.** Run `cargo test -p fishy-core logs::`. Expected: FAIL.
- [ ] **Step 3: Implement the items in Produces.** Use `tracing_appender::rolling::Builder` with `Rotation::DAILY`. The non-blocking wrapper goes outside `ScrubWriter`. Lines are formatted without ANSI colours.
- [ ] **Step 4: Run the tests again.** Same command. Expected: PASS.
- [ ] **Step 5: Commit.** `feat(core): daily log files through the scrubber, 7 days kept`

### Task 3: Logging in the app, and the Open logs folder command

**Files:**
- Modify: `app/src-tauri/src/lib.rs`, `app/src-tauri/src/downloads.rs`, `app/src-tauri/src/updates.rs`, `app/src-tauri/src/play.rs`, `app/src-tauri/Cargo.toml` (`tracing`)
- Create: `app/src-tauri/src/data.rs` (the Account commands live here)

**Interfaces:**
- Consumes: `fishy_core::logs::{init, default_dir}`
- Produces:
  - the `#[tauri::command] fn open_logs_folder() -> Result<(), String>` command
  - `fn logs_error() -> String`, which returns the exact Open-logs error text
  - the `LogGuard` held in app state for the app's lifetime

- [ ] **Step 1: Write the failing test** in `data.rs`: `open_logs_error_is_plain` checks that `logs_error()` equals the Global Constraints text.
- [ ] **Step 2: Run it and see it fail.** Run `cargo test -p fishy-client data::`, using the app crate's package name from `app/src-tauri/Cargo.toml`. Expected: FAIL.
- [ ] **Step 3: Implement the command and the log lines:**
  - Call `init` first in `setup`. If it errors, the app runs without logs.
  - Log the app start with the version from `app.package_info().version`.
  - Log these steps:
    - sign-in state changes (only the state name)
    - logout
    - Library refresh (count, plus how long it took in seconds)
    - install, update, pause, resume, cancel and uninstall (app name, sizes, result)
    - each command error, logged as its `user_message()` string
  - `open_logs_folder` runs `create_dir_all(default_dir())`, then `std::process::Command::new("explorer.exe").arg(dir).spawn()`. Any error becomes `logs_error()`.
  - Register the command.
- [ ] **Step 4: Run the test again and check by hand.** Run the same test; expected PASS. Then run `cargo clippy --workspace --all-targets` with no warnings, and `grep -rn "info!\|warn!" app/src-tauri/src` to check by eye that no line formats a `Secret`, a token, an `EpicError`'s raw text or an account ID.
- [ ] **Step 5: Commit.** `feat(app): write logs at key steps; Open logs folder command`

### Task 4: Wiping the data folder

**Files:**
- Create: `crates/fishy-core/src/wipe.rs`
- Modify: `crates/fishy-core/src/lib.rs`

**Interfaces:**
- Produces:
  - `pub struct WipeReport { pub failed: Vec<PathBuf> }`
  - `pub fn wipe_dir(root: &Path) -> WipeReport`, which deletes everything *inside* `root`, depth first. A `.log` file that can't be deleted is emptied with `set_len(0)` and isn't counted as failed. A missing `root` gives an empty report.

- [ ] **Step 1: Write the failing tests:**
  - `wipes_everything_inside`: `settings.json`, `library.json`, `manifests\a.manifest`, `downloads\x\job.json` and `logs\fishy-client.2026-11-21.log` are all gone, `root` still exists, and `failed` is empty.
  - `missing_root_is_fine`: `failed` is empty.
  - `#[cfg(windows)] a_locked_file_is_reported_and_the_rest_goes`: hold `downloads\x\big.bin` open with `OpenOptionsExt::share_mode(0)`. `failed` equals `[that path]`, and every other file is gone.
  - `#[cfg(windows)] an_open_log_file_is_emptied`: hold the log open with `share_mode(0)`, wipe, drop the handle. The file exists with length 0 and `failed` is empty. If `set_len` also fails under share mode 0, open it with `FILE_SHARE_WRITE` instead, which is how `tracing-appender` holds it.
- [ ] **Step 2: Run them and see them fail.** Run `cargo test -p fishy-core wipe::`. Expected: FAIL.
- [ ] **Step 3: Implement `wipe_dir`.**
- [ ] **Step 4: Run the tests again.** Expected: PASS.
- [ ] **Step 5: Commit.** `feat(core): wipe the data folder, emptying an open log`

### Task 5: The `delete_all_data` command

**Files:**
- Modify: `app/src-tauri/src/data.rs`, `app/src-tauri/src/lib.rs`, `app/src-tauri/src/downloads.rs`, `app/src-tauri/Cargo.toml` (`tauri-plugin-autostart = "2"`), `app/src-tauri/capabilities/*.json` (autostart permissions)

**Interfaces:**
- Consumes: `Downloads::pause_all`, `Downloads::take_jobs_for_wipe`, `AuthManager::logout`, `LibraryCache::clear`, `uninstall::{uninstall, discard_download}`, `wipe_dir`, `SettingsStore::default_location`
- Produces:
  - `#[derive(Serialize)] pub struct DeleteReport { pub failed: usize }`
  - `#[tauri::command] async fn delete_all_data(also_games: bool, …) -> Result<DeleteReport, String>`, which returns `Ok` even when some files failed
  - `fn delete_message(failed: usize) -> Option<String>`, which returns the exact "Some files couldn't be deleted (N)…" text, or `None` when 0
  - `pub fn take_jobs_for_wipe(&self) -> Vec<JobInfo>` on `Downloads`, which removes every job from the queue (after `pause_all`) and returns them, so nothing restarts
  - an async core helper, `pub async fn delete_everything(store: &InstallStore, data_root: &Path, jobs: Vec<JobInfo>, also_games: bool) -> usize`. It does steps 3 and 5 from §6f.1 and returns the failure count, so it can be tested without Tauri. It lives in `crates/fishy-core/src/wipe.rs`.

- [ ] **Step 1: Write the failing tests:**
  - core, `delete_everything_keeps_game_folders_when_not_ticked`:
    - Make a temp data root whose store has one finished game installed at `tmp\games\A`, plus one paused job writing to `tmp\games\B`.
    - With `also_games = false`, the data root is empty, and `games\A` and `games\B` still exist.
  - core, `delete_everything_removes_games_and_half_downloads_when_ticked`: with the same setup and `true`, `games\A` and `games\B` are gone and the result is 0.
  - app, `delete_message_wording`: `delete_message(0)` is `None`, and `delete_message(3)` is `Some("Some files couldn't be deleted (3). Close any running games and try again.")`.
- [ ] **Step 2: Run them and see them fail.** Expected: FAIL.
- [ ] **Step 3: Implement the command.** The order follows §6f.1:
  1. `pause_all`, then `take_jobs_for_wipe`, then clear `prepared`
  2. `auth.manager.logout()`, then `library.cache.clear()`, then `auth.set(...)`
  3. `delete_everything`
  4. `app.autolaunch().disable()`, ignoring any error
  5. reset the shared speed limiter to `None` (once Task 8 exists; leave a `// Task 8` marker until then)

  Register `tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None)` and the command.
- [ ] **Step 4: Run the tests again.** Run `cargo test --workspace` and clippy. Expected: PASS, no warnings.
- [ ] **Step 5: Commit.** `feat(app): Delete all my data (optionally the installed games)`

### Task 6: Account section in the UI

**Files:**
- Create: `app/src/lib/settings/DeleteDataBox.svelte`, `app/src/lib/settings/data.ts`
- Modify: `app/src/lib/settings/SettingsPage.svelte`, `app/src/lib/settings/SettingsPage.test.ts`, `app/src/lib/shell/AppShell.svelte` (remove Log out and `logoutError`), plus the shell's test if one checks Log out
- Test: `app/src/lib/settings/DeleteDataBox.test.ts`

**Interfaces:**
- Consumes: the commands `open_logs_folder` and `delete_all_data { alsoGames }`, which returns `{ failed }`; `signOut()`; the `installed` store and its sizes, as used in the Library
- Produces:
  - `openLogsFolder(): Promise<string | null>`, which returns an error message or null
  - `deleteAllData(alsoGames: boolean): Promise<{ failed: number } | string>`. On success it sets `authState` to signed-out (re-read with `auth_state`), resets the library and resets settings.

- [ ] **Step 1: Write the failing tests:**
  - `account_section_has_log_out_logs_and_delete`: the "Account" heading and the "Log out", "Open logs folder" and "Delete all my data…" buttons are present. Clicking Log out calls `logout`.
  - `open_logs_failure_shows_the_message`: the error text appears with `role="alert"`.
  - DeleteDataBox:
    - `tick_box_hidden_with_no_games`
    - `tick_box_shows_count_and_size`: with 3 games totalling 42 GB, it reads "Also delete my installed games (3 games, 42 GB)", unticked, and the "Your game folders stay on disk…" note shows; ticking the box hides the note.
    - `cancel_does_nothing`: no `delete_all_data` call is made.
    - `delete_sends_the_tick_and_disables_buttons`: press Delete. While the call is still pending, Delete and Cancel are disabled. The call gets `alsoGames: false`, exactly once, and afterwards `authState` is signed-out.
    - `partial_failure_shows_the_message`: `{ failed: 2 }` shows "Some files couldn't be deleted (2). Close any running games and try again."
  - Shell: no "Log out" button in the sidebar.
- [ ] **Step 2: Run them and see them fail.** Run `npm.cmd test -- settings shell`. Expected: FAIL.
- [ ] **Step 3: Implement.** Reuse the existing modal pattern from the Play warning box. Use the app's existing size formatter for GB.
- [ ] **Step 4: Run the tests again.** Run `npm.cmd test` and `npm.cmd run check`. Expected: all PASS, and check is clean.
- [ ] **Step 5: Commit.** `feat(ui): Account section with Log out, Open logs folder and Delete all my data`

## Half 1b: the four new settings

### Task 7: New settings fields and the default install folder

**Files:**
- Modify: `crates/fishy-core/src/settings.rs`, `crates/fishy-core/src/install/space.rs`, `app/src-tauri/src/lib.rs` (`install_path`, `install_options`), `app/src/lib/settings/settings.ts`, `app/src/lib/settings/SettingsPage.test.ts` (the expected saved objects now include the new defaults), the Install box component and its test

**Interfaces:**
- Produces:
  - `Settings` gains `install_folder: Option<String>`, `speed_limit_mbps: Option<u32>` and `reduce_motion: bool`. It is no longer `Copy`; it is still `Clone`.
  - `pub fn install_root(saved: Option<&str>) -> (PathBuf, bool)` in `space.rs` returns the root to use, and whether the saved one was unavailable. A saved folder counts as unavailable when its drive root (`Path::ancestors().last()`) doesn't exist; a missing folder on an existing drive is fine, because the install creates it.
  - `InstallOptions` gains `note: Option<String>`, which holds the "Your default folder isn't available…" text when it applies. The TS type gains `note?: string | null`.

- [ ] **Step 1: Write the failing tests:**
  - core:
    - `old_settings_file_loads_with_new_defaults`: `{"auto_update": true}` loads as `install_folder: None`, `speed_limit_mbps: None`, `reduce_motion: false`.
    - Extend `settings_round_trip` to cover all five fields.
  - core, `install_root`:
    - `None` gives `(default_install_root(), false)`.
    - `Some(tmp)` gives `(tmp, false)`.
    - `Some(r"Q:\Games")`, on a drive that doesn't exist (pick the first unused letter from Z down to D in the test), gives `(default_install_root(), true)`.
  - UI: the Install box shows `note` when one is returned.
- [ ] **Step 2: Run them and see them fail.**
- [ ] **Step 3: Implement.** `install_path` takes the parent if one was given. Otherwise it uses `install_root(settings.install_folder)`, and `install_options` fills `note` when the saved folder was unavailable. Add `AppSettings` to `install_options` and `install_start`.
- [ ] **Step 4: Run the tests again.** Run `cargo test --workspace` and `npm.cmd test`. Expected: PASS.
- [ ] **Step 5: Commit.** `feat: settings gain install folder, speed limit and reduce motion; Install uses the default folder`

### Task 8: Speed limiter

**Files:**
- Create: `crates/fishy-core/src/install/limit.rs`
- Modify: `crates/fishy-core/src/install/mod.rs`, `crates/fishy-core/src/install/engine.rs` (`RunOptions` loses `Copy` and gains `limiter: Option<Arc<SpeedLimiter>>`; call `take` before each `download_chunk`), `crates/fishy-core/Cargo.toml` (tokio `test-util` in dev-dependencies), `app/src-tauri/src/downloads.rs` and `app/src-tauri/src/updates.rs` (one shared `Arc<SpeedLimiter>`, set at startup and in `settings_set`), `app/src-tauri/src/data.rs` (replace the Task 5 marker)

**Interfaces:**
- Produces:
  - `pub struct SpeedLimiter` with:
    - `pub fn new() -> Self`
    - `pub fn set(&self, bytes_per_sec: Option<u64>)`
    - `pub async fn take(&self, bytes: u64)`
  - A token bucket with a capacity of one second at the current rate. It reads the rate on every `take`, and with `None` it returns at once. A `take` larger than the capacity waits for its debt to be paid, so large chunks still work.
  - `pub fn limit_bytes(s: &Settings) -> Option<u64>`, which is `speed_limit_mbps × 1_000_000`

- [ ] **Step 1: Write the failing tests** (`#[tokio::test(start_paused = true)]`):
  - `no_limit_never_waits`: 100 × `take(1_000_000)` with `Instant` elapsed of 0.
  - `two_mb_at_one_mb_per_sec_takes_about_two_seconds`: `set(Some(1_000_000))`, then 4 × `take(500_000)`. Elapsed is between 1.9 s and 2.1 s (the bucket starts empty).
  - `a_change_applies_to_the_next_take`: at 1 MB/s, `take(1_000_000)`, then `set(None)`, then `take(5_000_000)` returns without waiting further.
  - `limit_bytes_reads_the_setting`: 10 gives `Some(10_000_000)`, and `None` gives `None`.
  - engine: the existing tests still pass with `limiter: None`, and one extra wiremock test, `engine_respects_the_limiter`, runs a basic install with a limiter at a tiny rate under paused time and checks that the elapsed time is at least the expected seconds.
- [ ] **Step 2: Run them and see them fail.** Run `cargo test -p fishy-core install::limit`. Expected: FAIL.
- [ ] **Step 3: Implement.** Use `tokio::sync::Mutex` for the bucket state and `tokio::time::{Instant, sleep}`. The app builds the limiter in `setup` from `limit_bytes(&store.load())`.
- [ ] **Step 4: Run the tests again.** Run `cargo test --workspace` and clippy. Expected: PASS, no warnings.
- [ ] **Step 5: Commit.** `feat: download speed limit, applied live`

### Task 9: Reduce motion follows the setting

**Files:**
- Modify: `app/src/lib/motion.ts`, `app/src/lib/motion.test.ts`

**Interfaces:**
- Consumes: the `settings` store
- Produces: `reducedMotion`, still a `Readable<boolean>` with the same name, so `FishLoader` and `FishProgressBar` don't change. It is now derived from the media query store and `$settings.reduce_motion`.

- [ ] **Step 1: Write the failing tests:**
  - `setting_on_forces_reduced_motion`: the media query says false and `reduce_motion` is true, so the value is true.
  - `setting_off_follows_windows`: the media query is true, so the value is true; the media query is false, so the value is false.
- [ ] **Step 2: Run them and see them fail.** Run `npm.cmd test -- motion`. Expected: FAIL.
- [ ] **Step 3: Implement** using a Svelte `derived` store.
- [ ] **Step 4: Run the tests again.** Expected: PASS.
- [ ] **Step 5: Commit.** `feat(ui): Reduce motion setting`

### Task 10: Settings UI for the new controls, plus Start with Windows

**Files:**
- Modify: `app/src/lib/settings/SettingsPage.svelte`, `app/src/lib/settings/SettingsPage.test.ts`, `app/src/lib/settings/data.ts`, `app/src-tauri/src/data.rs` (`autostart_get`, `autostart_set`), `app/src-tauri/src/lib.rs`

**Interfaces:**
- Consumes:
  - `saveSettings`
  - `pick_folder`, which returns `string | null`
  - `install_root` (for showing the default: add `#[tauri::command] fn default_install_folder() -> String`)
- Produces:
  - `#[tauri::command] fn autostart_get(app) -> bool`, which returns false if it can't read
  - `#[tauri::command] fn autostart_set(app, on: bool) -> Result<bool, String>`, which returns the real state afterwards; on error it gives "Couldn't change Start with Windows."
  - in TS, `parseSpeed(text: string): number | null`, which accepts only whole numbers from 1 to 1000

- [ ] **Step 1: Write the failing tests:**
  - `sections_in_order`: the headings are "Downloads", "Appearance and startup" and "Account", in that order.
  - Install folder:
    - `install_folder_shows_default_then_change_and_use_default`: with no folder saved, the page shows the default path and no "Use default" button.
    - "Change…" with `pick_folder` returning `D:\Games` saves `install_folder: "D:\\Games"` and shows "Use default".
    - "Use default" saves `null`.
  - `picker_failure_says_so`: "Couldn't open the folder picker."
  - Speed limit:
    - `speed_switch_saves_10_by_default`: turning it on saves `speed_limit_mbps: 10`, and turning it off saves `null`.
    - `speed_box_refuses_bad_numbers`: for each of "", "2.5", "abc", "0" and "1001", the message is "Pick a number from 1 to 1000." and nothing is saved; "25" saves 25.
    - `parseSpeed` unit cases cover the same values.
  - `reduce_motion_switch_saves`
  - Start with Windows:
    - `start_with_windows_reads_the_real_state`: `autostart_get` gives true, so the switch is on, and clicking it calls `autostart_set { on: false }`.
    - On error, the page shows "Couldn't change Start with Windows." and the switch goes back.
- [ ] **Step 2: Run them and see them fail.** Run `npm.cmd test -- settings`. Expected: FAIL.
- [ ] **Step 3: Implement** the controls and the three commands. Register them, and add the autostart `allow-enable`, `allow-disable` and `allow-is-enabled` permissions only if the plugin's commands are called from JS. Calling them from Rust needs none.
- [ ] **Step 4: Run the tests again.** Run `npm.cmd test`, `npm.cmd run check`, `cargo test --workspace` and clippy. Expected: all PASS and clean.
- [ ] **Step 5: Commit.** `feat(ui): Settings for install folder, speed limit, reduce motion, Start with Windows`

### Task 11: Preview check, §12, push, hand check

**Files:**
- Modify: `docs/superpowers/specs/2026-11-20-fishy-client-design.md` §12

- [ ] **Step 1:** In the running dev server's browser preview (`http://localhost:1420/?mock=signedin`), open Settings. Check the three sections and the Delete box, both unticked and ticked, and that there are no console errors. Add `devMock.ts` handlers for the new commands if the page needs them.
- [ ] **Step 2:** Update §12:
  - "What's done" for Polish 1
  - the new test counts
  - a "Still to check by hand (milestone 7)" list:
    1. A log file appears; Open logs folder opens it; after signing in and refreshing, `Select-String -Path "$env:APPDATA\FishyClient\logs\*.log" -Pattern "eg1~|access_token=[^\[]"` finds nothing.
    2. The speed limit visibly slows a download, and lifting it speeds it up again.
    3. A changed default folder is used by the Install box.
    4. Reduce motion calms the fish.
    5. Start with Windows: the switch stays as set after a restart, and the app starts after signing out of Windows and back in.
    6. Delete all my data, unticked: you're logged out, `%APPDATA%\FishyClient` is empty apart from today's empty log, `cmdkey /list | findstr /i fishy` prints nothing, and the game folders still exist.
    7. The two items left from milestone 6: the warning's tick box, and settings staying as set after a restart.
  - Remove "log files…" and "delete all my data" from "Next steps".
- [ ] **Step 3: Commit and push.** `docs: §12 progress log for Polish 1`, then `git push`.
- [ ] **Step 4:** Hand check with the user in `npm.cmd run tauri dev`. Tick off what passed in §12, then commit and push again.
