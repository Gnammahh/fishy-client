# Manage Box Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every installed game gets a Manage box (Auto update, Priority download, Shortcut, Installation + Verify, Launch options), and Downloads gets Start now.

**Architecture:** Per-game choices live in a new `options` part of each `installed.json` record (Rust core). The Tauri side adds commands for options, Verify, shortcuts, Start now and the `--play` request; the page adds `ManageDialog.svelte` opened from the shared ⋯ menu snippet, a Start now box on the Downloads page, and a play-request store.

**Tech Stack:** Rust (fishy-core, Tauri 2), Svelte 5 + vitest, `windows` crate (IShellLinkW), `tauri-plugin-single-instance` 2, `sha1` (already a dependency).

**Spec:** `docs/superpowers/specs/2026-11-20-fishy-client-design.md` §6t (read it first; copy texts from it exactly).

## Global Constraints

- Version for the release: **1.9.0**.
- Launch options: at most **1000** characters (`chars().count()`), refused with exactly `Keep it under 1000 characters.`
- At most **2** downloads run at once; the normal queue starts a job only when **none** is downloading.
- Progress events: at most 4 a second (every 250 ms), like `EMIT_EVERY`.
- Logs (§12): no paths, no titles, no launch options, no tokens/account IDs/Epic error text. Allowed lines: `options saved`, `shortcut created`, `shortcut removed`, `verify: B of N files bad`, `start now: pause` / `start now: both`, `play request from shortcut`. App names are fine (existing logs use them).
- Disabled reasons (tooltips, exact): `Close the game first.` · `Finish or cancel its download first.` · `Another game is being checked.` · `Two games are already downloading.`
- Dialogs: before styling `ManageDialog` and the Start now box, check `git log` for the other session's dialog fix (dialogs overlapping the title bar, §6s) and copy its overlay rule (top offset / z-index). Don't edit `WhatsNewBox`, `ConfirmDialog`, `InstallDialog` while that session works on them.
- Commit messages end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Commands: Rust tests `cargo test --workspace` (from repo root); UI tests `npm.cmd test` (from `app/`); types `npm.cmd run check` (from `app/`).

## Review Focus

1. **A record rewritten by an update, a repair or a Verify repair keeps its options** (shortcuts, launch options, priority). Test in Task 1 (`finish_install_keeps_options`).
2. **Start now → Pause while the running job is an update or is being cancelled**: the paused job must come back Queued, never lost, and a cancel must still delete. Tests in Task 4 (`pause_mode_requeues_right_behind_the_chosen_job`, `start_now_refuses_a_job_being_cancelled`). Delete all my data (`stop_all`) must clear `requeue` so nothing restarts during a wipe (`stop_all_clears_requeue`).
3. **A shortcut file name already used by something that isn't ours** is never overwritten or deleted. Tests in Task 6 (`refuses_a_name_already_taken`, `remove_deletes_only_recorded_lnk_files`).
4. **`--play` with a name that isn't installed, or arguments with no name**: Fishy just opens. Test in Task 7 (`play_arg_parsing`), plus `play_request_for_unknown_game_is_ignored`.
5. **Verify while an update is waiting** must not queue a repair of the old version from the new build's CDN: refused with `An update is waiting. Update the game, then verify again.` Test in Task 5 (`repair_needs_the_installed_version`).

---

### Task 1: Per-game options in the installed record

**Files:**
- Modify: `crates/fishy-core/src/install/store.rs` (struct `InstalledGame`, `finish_install`, new methods, tests)
- Modify every `InstalledGame { … }` literal to add `options: GameOptions::default()`: `crates/fishy-core/src/launch/command.rs:138`, `crates/fishy-core/src/launch/mod.rs:125`, `app/src-tauri/src/epic_import.rs:321`, `app/src-tauri/src/lib.rs:710`, `app/src-tauri/src/updates.rs:189` (find more with `grep -rn "playtime_secs: 0"`).

**Interfaces:**
- Produces (in `fishy_core::install::store`):
  - `#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)] #[serde(default)] pub struct GameOptions { pub auto_update: Option<bool>, pub priority: bool, pub launch_args: String, pub launch_args_on: bool, pub shortcuts: Vec<PathBuf> }`
  - `InstalledGame.options: GameOptions` with `#[serde(default)]`
  - `pub const LAUNCH_ARGS_MAX: usize = 1000;` `pub const LAUNCH_ARGS_TOO_LONG: &str = "Keep it under 1000 characters.";`
  - `pub fn check_launch_args(text: &str) -> Result<(), &'static str>`
  - `InstallStore::set_options(&self, app: &str, auto_update: Option<bool>, priority: bool, launch_args: &str, launch_args_on: bool) -> io::Result<Option<InstalledGame>>` (None: not installed; keeps `shortcuts`)
  - `InstallStore::set_shortcuts(&self, app: &str, paths: Vec<PathBuf>) -> io::Result<()>` (no-op if not installed)

- [ ] **Step 1: Write the failing tests** in `store.rs` tests:
  - `options_default_in_old_records`: deserialize a record JSON without `options` → `options == GameOptions::default()`.
  - `finish_install_keeps_options`: install, `set_options(.., Some(false), true, "-windowed", true)`, `set_shortcuts(vec![p])`, then `finish_install` again (an update) → all five fields equal.
  - `set_options_keeps_shortcuts_and_ignores_missing_games`: shortcuts unchanged after `set_options`; `set_options("nope", …)` → `Ok(None)`.
  - `launch_args_limit`: `check_launch_args(&"a".repeat(1000))` ok; 1001 → `Err(LAUNCH_ARGS_TOO_LONG)`; 1000 `é` ok (characters, not bytes).
- [ ] **Step 2:** `cargo test -p fishy-core store::` → FAIL (no `GameOptions`).
- [ ] **Step 3:** Implement. `finish_install` carries `before.options` (clone) like `playtime_secs`. Both setters take `installed_lock()` and use `save_installed`. Add `options: GameOptions::default()` to the other literals.
- [ ] **Step 4:** `cargo test --workspace` → PASS.
- [ ] **Step 5:** Commit `feat(core): per-game options in installed records (spec §6t)`.

### Task 2: Launch options on the command line

**Files:** Modify `crates/fishy-core/src/launch/command.rs` (`build_command`, `Debug for LaunchCommand`, tests).

**Interfaces:**
- Consumes: `InstalledGame.options.launch_args`, `.launch_args_on` (Task 1).
- Produces: `LaunchCommand.user_args: usize` (how many trailing args are the user's; used only by `Debug`).

- [ ] **Step 1: Failing tests:**
  - `launch_options_go_last_when_on`: options `"-windowed \"-x y\""`, on → `cmd.args` ends with `["-windowed", "-x y"]`, after Epic's `-epicsandboxid=…` args.
  - `launch_options_ignored_when_off`: same text, off → neither appears.
  - `debug_hides_launch_options`: `format!("{cmd:?}")` contains `[user options]` and not `-windowed`.
- [ ] **Step 2:** `cargo test -p fishy-core launch::command` → FAIL.
- [ ] **Step 3:** In `build_command`, after the last `args.extend([...])`, when `launch_args_on`, extend with `split_args(&launch_args)` and set `user_args` to that count. `Debug` prints the first `len - user_args` args (still redacting `-AUTH_PASSWORD=`) then the string `[user options]` if `user_args > 0`.
- [ ] **Step 4:** `cargo test --workspace` → PASS.
- [ ] **Step 5:** Commit `feat(launch): per-game launch options (spec §6t)`.

### Task 3: Verify files (core)

**Files:**
- Create: `crates/fishy-core/src/install/verify.rs` (+ `pub mod verify;` in `install/mod.rs`)

**Interfaces:**
- Consumes: `crate::epic_import::FileCheck` (`good: Vec<(String /*sha1 hex*/, String /*name*/)>`, `bad_files`, `bad_bytes`), `file_path`, `Manifest.files` (`filename`, `size()`, `sha1: [u8; 20]`).
- Produces: `pub fn verify(root: &Path, m: &Manifest, stop: &AtomicBool, on_progress: &mut dyn FnMut(usize, usize)) -> Option<FileCheck>` — `None` when stopped. `on_progress(done, total)` after each file. The caller passes a manifest already filtered with `only_parts`.

- [ ] **Step 1: Failing tests** (tempfile + `manifest::testbuild` helpers, as `epic_import` tests do):
  - `all_good_files_pass`: every file written with its bytes → `bad_files == 0`, `good.len() == files.len()`.
  - `missing_and_wrong_size_are_bad_without_reading`: one deleted, one truncated → `bad_files == 2`, `bad_bytes` = their manifest sizes.
  - `same_size_different_bytes_is_bad`: flip one byte → `bad_files == 1`.
  - `stop_returns_none`: `stop` set before the call → `None`.
  - `progress_counts_every_file`: last call is `(n, n)`.
- [ ] **Step 2:** `cargo test -p fishy-core verify` → FAIL.
- [ ] **Step 3:** Implement: per file, `metadata` (missing/not file/wrong len → bad); else read with a reused 1 MiB buffer into `sha1::Sha1`, checking `stop` between blocks; compare to `f.sha1`. Unsafe names (`file_path` Err) count as bad.
- [ ] **Step 4:** `cargo test --workspace` → PASS.
- [ ] **Step 5:** Commit `feat(core): verify a game's files by SHA-1 (spec §6t)`.

### Task 4: Priority and Start now in the Downloads manager

**Files:** Modify `app/src-tauri/src/downloads.rs`, `app/src-tauri/src/lib.rs` (register commands).

**Interfaces:**
- Consumes: `InstalledGame.options.priority` (Task 1).
- Produces:
  - `Job.priority: bool`, `Job.requeue: bool` (private-to-module fields like `stop`).
  - `Inner::prioritize(&mut self, app: &str)` — a Queued priority job moves to just before the first Queued non-priority job that is ahead of it.
  - `Inner::set_priority(&mut self, app: &str, on: bool)` — sets the flag; `on` → `prioritize`.
  - `pub enum StartNow { Pause, Both }`; `Inner::start_now(&mut self, app: &str, mode: StartNow) -> Result<Option<usize>, &'static str>` — `Ok(None)`: the queue will start it (it's now first among Queued; with Pause the running job got `requeue = true` and its stop token cancelled); `Ok(Some(i))`: start index `i` now (Both). Errors: not Queued or `wiping` → `"That game isn't waiting."`; two downloading → `"Two games are already downloading."`.
  - `Downloads::start_now(&self, app: &AppHandle, app_name: &str, mode: StartNow) -> Result<DownloadsView, String>`; `Downloads::set_priority(&self, app: &AppHandle, app_name: &str, on: bool)`.
  - Tauri command `download_start_now(app_name: String, mode: String /* "pause" | "both" */) -> Result<DownloadsView, String>`.
  - Split `pump` into `next_to_start` + a private `start_at(&self, app, i)` used by both `pump` and Both.

- [ ] **Step 1: Failing tests** (module tests, `Inner` only, using the existing `job()` helper):
  - `priority_job_goes_before_waiting_non_priority_ones`: Queued a (normal), then b (priority), then c (priority), each followed by `prioritize` → Queued order is [b, c, a] (in front of normal ones, behind earlier priority ones).
  - `turning_priority_on_moves_a_waiting_job_forward`; turning off doesn't move it.
  - `start_now_with_nothing_running_returns_none_and_puts_it_first`.
  - `both_needs_exactly_one_running`: one Downloading + chosen Queued → `Ok(Some(i))`; two Downloading → `Err("Two games are already downloading.")`.
  - `pause_mode_requeues_right_behind_the_chosen_job`: after `start_now(Pause)` the running job has `requeue`; simulate its finish with `InstallError::Stopped` via the extracted pure `Inner::stopped(&mut self, app)` → state Queued, placed right after the chosen job; `next_to_start()` is the chosen job.
  - `start_now_refuses_a_job_being_cancelled` (state Removing) and a Paused job.
  - `stop_all_clears_requeue`: requeue set, `stop_all()` → `stopped()` gives Paused.
  - `queue_never_starts_a_third`: two Downloading + Queued → `next_to_start() == None`.
- [ ] **Step 2:** `cargo test -p fishy-client downloads` → FAIL.
- [ ] **Step 3:** Implement. Extract the `Err(InstallError::Stopped)` arm of `finish` into `Inner::stopped(app)` (requeue → Queued, inserted right after the first Queued job, `requeue = false`; else Paused). `enqueue` and `resume` read `priority` from `self.store.installed()` for that app, set it on the job and call `prioritize`. Log `start now: pause` / `start now: both`.
- [ ] **Step 4:** `cargo test --workspace` → PASS.
- [ ] **Step 5:** Commit `feat(downloads): priority games and Start now (spec §6t)`.

### Task 5: Options, Verify and repair commands

**Files:**
- Create: `app/src-tauri/src/verify.rs`
- Modify: `app/src-tauri/src/lib.rs` (manage `verify::AppVerify`, register commands, add `game_options_set`)

**Interfaces:**
- Consumes: Task 1 setters, Task 3 `verify`, Task 4 `Downloads::set_priority`, `epic_import::save_repair`, `only_parts`, `downloads.client.manifest_info`, `running::AppRunning::contains`, `install.checking.is_busy`, `downloads.has_job`, `downloads.is_wiping`.
- Produces:
  - `game_options_set(app_name, auto_update: Option<bool>, priority: bool, launch_args: String, launch_args_on: bool) -> Result<InstalledGame, String>` — `check_launch_args` first; not installed → `InstallError::NotInstalled.user_message()`; calls `downloads.set_priority`; emits `installed-changed`; logs `options saved`.
  - `pub struct AppVerify { current: Mutex<Option<(String, Arc<AtomicBool>)>>, bad: Mutex<HashMap<String, FileCheck>> }`
  - `pub fn verify_refusal(running: bool, has_job: bool, busy_other: bool, wiping_or_checking: bool) -> Option<&'static str>` → `Close the game first.` / `Finish or cancel its download first.` / `Another game is being checked.` (pure, tested).
  - `verify_start(app_name) -> Result<(), String>`; events `verify-progress {app_name, done, total}` (≤ 4/s, plus the final count) and `verify-done {app_name, result}` where `result` is `{kind:"good"} | {kind:"bad", files, bytes} | {kind:"stopped"} | {kind:"error", message}`. Logs `verify: B of N files bad`.
  - `verify_stop(app_name)`.
  - `pub fn repair_needs_same_version(installed: &str, epic: &str) -> Result<(), &'static str>` → `An update is waiting. Update the game, then verify again.`
  - `verify_repair(app_name) -> Result<DownloadsView, String>`: takes the saved `FileCheck`; `manifest_info` → if different build → error above; job: `JobInfo { imported: true, update_from: None, parts: rec.parts, parent: rec.parent, base_urls, download_size: sum of chunk sizes of the parts manifest, install_size, … }`; `save_repair(store, &job, &saved_manifest_bytes, &check)`; `enqueue(only_parts(manifest))`. Expired sign-in handled like `update_start` (`session::expire`).

- [ ] **Step 1: Failing tests** in `verify.rs`: `verify_refusal` order (running wins over job over busy); `repair_needs_the_installed_version`.
- [ ] **Step 2:** `cargo test --workspace verify` → FAIL.
- [ ] **Step 3:** Implement; run `verify` in `spawn_blocking`; clear `current` when done; keep `bad[app]` only for a bad result.
- [ ] **Step 4:** `cargo test --workspace` → PASS; `cargo build -p fishy-client` compiles.
- [ ] **Step 5:** Commit `feat(app): options, verify and repair commands (spec §6t)`.

### Task 6: Shortcuts

**Files:**
- Create: `app/src-tauri/src/shortcuts.rs`
- Modify: `app/src-tauri/Cargo.toml` (`[target.'cfg(windows)'.dependencies] windows = { version = "0.62", features = ["Win32_System_Com", "Win32_UI_Shell", "Win32_Foundation", "Win32_System_Com_StructuredStorage"] }` — the version already in `Cargo.lock`), `app/src-tauri/src/lib.rs` (`uninstall_game` removes shortcuts before forgetting the record; register commands)

**Interfaces:**
- Consumes: `InstallStore::set_shortcuts` (Task 1).
- Produces:
  - `pub fn shortcut_name(title: &str, app_name: &str) -> String` (removes `\ / : * ? " < > |` and control chars, trims; empty → `app_name`; adds `.lnk`).
  - `pub fn taken_by_other(path: &Path, recorded: &[PathBuf]) -> bool`.
  - `pub fn remove_recorded(paths: &[PathBuf]) -> usize` (deletes each that still exists, is a file and ends `.lnk`; returns how many).
  - `fn write_lnk(path: &Path, target: &Path, args: &str, icon: &Path, workdir: &Path) -> Result<(), String>` (Windows: `CoInitializeEx` apartment, `CoCreateInstance(ShellLink)`, `SetPath`/`SetArguments`/`SetIconLocation(icon, 0)`/`SetWorkingDirectory`, `IPersistFile::Save`).
  - `fn known_folder(id) -> Option<PathBuf>` via `SHGetKnownFolderPath(FOLDERID_Desktop | FOLDERID_Programs)`.
  - Commands `shortcut_create(app_name) -> Result<InstalledGame, String>` (desktop; error `There's already a shortcut called {Title} on your desktop.`), `shortcut_add_start_menu(app_name) -> Result<InstalledGame, String>` (into `Programs\Fishy Client\`, creating the folder), `shortcut_remove(app_name) -> Result<InstalledGame, String>`. Target `current_exe()`, args `--play {app_name}` (quote it if it has spaces), icon = the game's exe via `file_path(install_path, executable)`. Logs `shortcut created` / `shortcut removed`.

- [ ] **Step 1: Failing tests:** `shortcut_name_cleans_titles` (`"Sol: Cesto?"` → `"Sol Cesto.lnk"`, `"  "` → `"Tide.lnk"`); `refuses_a_name_already_taken` (existing file not recorded → true; recorded → false; missing → false); `remove_deletes_only_recorded_lnk_files` (a recorded `.txt` and a recorded folder survive; a recorded `.lnk` goes).
- [ ] **Step 2:** `cargo test --workspace shortcuts` → FAIL.
- [ ] **Step 3:** Implement; `write_lnk` and `known_folder` are `#[cfg(windows)]` (other targets return an error string).
- [ ] **Step 4:** `cargo test --workspace` → PASS. Quick real check: `cargo test -p fishy-client -- --ignored write_lnk_makes_a_file` (an `#[ignore]` test writing into a temp folder, asserting the file exists).
- [ ] **Step 5:** Commit `feat(app): desktop and Start menu shortcuts (spec §6t)`.

### Task 7: `--play` from a shortcut

**Files:**
- Modify: `app/src-tauri/Cargo.toml` (`tauri-plugin-single-instance = "2"`), `app/src-tauri/src/lib.rs` (plugin registered **first**; `AppPlayRequest`; command `play_request_take`), `app/src-tauri/capabilities/*.json` only if the plugin needs a permission (it doesn't for Rust-only use).
- Create: `app/src-tauri/src/play_request.rs`

**Interfaces:**
- Produces:
  - `pub fn play_arg(args: &[String]) -> Option<String>` (value after `--play`, non-empty).
  - `pub struct AppPlayRequest(Mutex<Option<String>>)`; `fn accept(app: &AppHandle, args: &[String])`: `play_arg` → must be in `installed()` (else ignored) → store it, log `play request from shortcut`, emit `play-request` with the name.
  - Single-instance callback: `accept(args)` and unminimize + show + focus the `main` window. Setup: `accept(std::env::args())`.
  - Command `play_request_take() -> Option<String>` (takes it out).
- [ ] **Step 1: Failing tests:** `play_arg_parsing` (`["fishy.exe","--play","Tide"]` → `Some("Tide")`; `["fishy.exe","--play"]` → None; `["fishy.exe"]` → None; `["x","--play",""]` → None); `play_request_for_unknown_game_is_ignored` (pure helper `fn wanted(name: &str, installed: &[InstalledGame]) -> bool`).
- [ ] **Step 2:** FAIL. **Step 3:** implement. **Step 4:** `cargo test --workspace` → PASS.
- [ ] **Step 5:** Commit `feat(app): start a game from a shortcut with --play (spec §6t)`.

### Task 8: Page helpers and the auto-update rule

**Files:** Modify `app/src/lib/install/installs.ts` (+ `installs.test.ts`), `app/src/lib/library/library.ts` (+ `library.test.ts`), `app/src/lib/settings/SettingsPage.svelte` (help text), `app/src/lib/auth/devMock.ts` (mock the new commands).

**Interfaces:**
- Consumes: commands from Tasks 4–7.
- Produces (in `installs.ts`):
  - `type GameOptions = { auto_update: boolean | null; priority: boolean; launch_args: string; launch_args_on: boolean; shortcuts: string[] }`; `InstalledGame.options?: GameOptions`.
  - `wantsAutoUpdate(g: InstalledGame, settingsDefault: boolean): boolean` (= `g.options?.auto_update ?? settingsDefault`).
  - `autoUpdate(latest, settingsDefault: boolean)` uses it; `library.ts` calls it on every online refresh / game close (no longer only when `settings.auto_update`).
  - `setGameOptions(app, o: Omit<GameOptions,"shortcuts">): Promise<string | null>` (updates `installed` store on success).
  - `type VerifyState = { app: string; done: number; total: number } | { app: string; result: { kind: "good" } | { kind: "bad"; files: number; bytes: number } | { kind: "stopped" } | { kind: "error"; message: string } }`; store `verifying: Writable<VerifyState | null>` fed by `verify-progress` / `verify-done` listeners added in `initInstalls`.
  - `verifyStart(app)`, `verifyStop(app)`, `verifyRepair(app)`, `createShortcut(app)`, `addStartMenu(app)`, `removeShortcut(app)`: each `Promise<string | null>` (error text), updating `installed`/`downloads` from the reply.
  - `startNow(app, mode: "pause" | "both"): Promise<string | null>`.
  - Store `playRequest: Writable<string | null>`; `initInstalls` takes `play_request_take` once and listens to `play-request`.
- [ ] **Step 1: Failing tests:** `wantsAutoUpdate` (null follows default both ways; own true/false wins); `autoUpdate` skips a game whose own switch is off while Settings is on and updates one whose own is on while Settings is off; a `verify-progress` event sets `verifying`; `play-request` event sets `playRequest`.
- [ ] **Step 2:** `npm.cmd test -- installs library` → FAIL. **Step 3:** implement. Settings help text for Auto update: `For every game you haven't set in its Manage box.`
- [ ] **Step 4:** `npm.cmd test` and `npm.cmd run check` → PASS.
- [ ] **Step 5:** Commit `feat(ui): helpers for the Manage box; per-game auto update (spec §6t)`.

### Task 9: The Manage box

**Files:**
- Create: `app/src/lib/library/ManageDialog.svelte`, `app/src/lib/library/ManageDialog.test.ts`
- Modify: `app/src/lib/library/LibraryPage.svelte` (`moreMenu`: **Manage…** first; drop the Size/Played/Epic lines; state `manageFor: Game | null`; render `ManageDialog`; its Uninstall sets `confirmUninstall` after closing)

**Interfaces:**
- Consumes: Task 8 helpers and stores, `formatBytes`, `formatPlaytime`, `openGameFolder`, `isRunning`, `sharedWithEpic`.
- Produces: `ManageDialog` props `{ game: Game; installed: InstalledGame; running: boolean; hasDownload: boolean; sharedWithEpic: boolean; onuninstall: () => void; onclose: () => void }`.
- Texts: copy every label/help text from spec §6t "What the user sees" exactly. Look: `role="dialog"`, `aria-modal`, `aria-labelledby` the title; title in the pixel font; rows as dark panels (`#0b0f14`-like, 2 px aqua-tinted border), switches as `<input type="checkbox" role="switch">` styled aqua; Esc, ✕ and a click on the overlay close it. Shortcut row text: one recorded path → `On your desktop`, two → `On your desktop and in the Start menu`. Launch options text saves 500 ms after typing stops and on blur; over 1000 characters shows the error and doesn't save.
- [ ] **Step 1: Failing tests** (`mockIPC`, as `DownloadsPage.test.ts`):
  - the ⋯ menu shows `Manage…` first, opens a dialog titled with the game's name, from the card and from `GamePage`;
  - five rows present; Auto update shows the Settings value when `options.auto_update` is null, and flipping it sends `game_options_set` with `auto_update: false/true`;
  - priority and launch options switch save; typing 1001 chars shows `Keep it under 1000 characters.` and sends nothing;
  - Create → `shortcut_create`, then `Also add it to the Start menu?` → Yes → `shortcut_add_start_menu`; with shortcuts recorded the button is Remove → `shortcut_remove`;
  - Verify disabled with the exact reasons (running / hasDownload / another game verifying); progress text `Checking files… 1,234 / 21,000`; bad result text `12 files are missing or damaged (340 MB).` and Download them → `verify_repair`; good → `All files are fine.`;
  - Uninstall closes the box and opens the existing confirm box; Esc closes.
- [ ] **Step 2:** `npm.cmd test -- ManageDialog GamePage` → FAIL. **Step 3:** implement (check the dialog fix first, see Global Constraints).
- [ ] **Step 4:** `npm.cmd test` and `npm.cmd run check` → PASS.
- [ ] **Step 5:** Commit `feat(ui): Manage box for each game (spec §6t)`.

### Task 10: Start now on the Downloads page, and Play from a shortcut

**Files:** Modify `app/src/lib/install/DownloadsPage.svelte` (+ test), `app/src/lib/shell/AppShell.svelte` (switch to Library when `playRequest` is set), `app/src/lib/library/LibraryPage.svelte` (+ `library` tests or a new `LibraryPage` play-request test): an `$effect` that, once the library is ready and the game is in it and installed, calls `askPlay(game)` and sets `playRequest` to null.

**Interfaces:** Consumes `startNow`, `playRequest` (Task 8).

- [ ] **Step 1: Failing tests:**
  - a queued item has **Start now**; with nothing downloading it sends `download_start_now` `{mode:"pause"}` without a box (mode doesn't matter then; use `"pause"`);
  - with one downloading it opens a box with `Pause Title a` and `Download both`, each sending its mode;
  - with two downloading the button is disabled with `Two games are already downloading.`;
  - setting `playRequest` to an installed game's name shows the Library and invokes `play_game` for it (or opens the outdated box when an update waits); an unknown name is cleared without playing.
- [ ] **Step 2:** FAIL. **Step 3:** implement. **Step 4:** `npm.cmd test`, `npm.cmd run check` → PASS.
- [ ] **Step 5:** Commit `feat(ui): Start now in Downloads; play from a shortcut (spec §6t)`.

### Task 11: Notes, progress log, release prep

**Files:** `app/src/lib/notes/release-notes.json` (new top entry `1.9.0`), spec §12 progress log ("Version 1.9.0 …" with test counts) and a "Still to check by hand (version 1.9.0)" list copied from §6t's By hand line.

- [ ] **Step 1:** Add the 1.9.0 notes (simple words, one item per row/feature: Manage box, Verify, shortcuts, launch options, auto update per game, priority + Start now).
- [ ] **Step 2:** `cargo test --workspace` and `npm.cmd test` → PASS; record the counts in §12.
- [ ] **Step 3:** Commit `docs: 1.9.0 notes and progress log`.
- Then (outside this plan's tasks, per the session task): a fresh whole-branch review on the most capable model, fixes test-first, release per `docs/RELEASING.md`, push `feat/import` and `master`, hand check with the user.
