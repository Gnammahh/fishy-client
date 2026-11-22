# Installing into folders that need administrator rights Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Games can be installed into `C:\Program Files\Epic Games` (or any folder that needs admin rights) after one Windows permission prompt per game folder, like Epic's launcher.

**Architecture:** A core module `fishy_core::install::elevate` holds the pure rules (path safety, argument parsing, exit-code messages, icacls arguments) plus two Windows calls: `run_helper` (the admin copy's work) and `ask_for_folder` (`ShellExecuteExW` "runas" on the current exe with `--make-game-folder <path>`, waiting for the exit code). `main.rs` handles the argument before Tauri. `install_options`/`install_start` use it instead of refusing. UI: the "Windows will ask…" note and **Use Epic's folder** buttons.

**Tech Stack:** Rust (windows-sys 0.61: `Win32_UI_Shell`, `Win32_System_Threading`, `Win32_Foundation`), Tauri 2, Svelte 5, vitest.

**Spec:** `docs/superpowers/specs/2026-11-20-fishy-client-design.md` §6k (and §6g.3, §6c).

## Global Constraints

- Argument exactly `--make-game-folder <path>` (two arguments after the exe, nothing else). Exit codes: 0 done, 2 refused path, 3 couldn't create or set permissions; declined prompt = 1223 (`ERROR_CANCELLED`).
- icacls: `%SystemRoot%\System32\icacls.exe`, args `[<path>, "/grant", "*S-1-5-32-545:(OI)(CI)F"]`.
- Copy: note `Windows will ask for permission once to create this folder.`; declined `Fishy Client needs your permission to use that folder. Pick a different folder.`; failed `Couldn't create {folder}. Pick a different folder.`; button `Use Epic's folder`.
- Epic's folder = `%ProgramFiles%\Epic Games`; the button shows only if it exists.
- Logs: `install: asking for permission to create a game folder`, `install: permission declined`, `install: game folder created with permission`; never the path.
- Release as 1.2.0 per `docs/RELEASING.md` (notes entry first). Commits end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. A path whose parent doesn't exist (`C:\Program Files\Nope\Game`): refused before asking (Task 1 `missing_parent_refused`).
2. The game folder already exists with files (a reinstall into Epic's own copy): refused, never re-permissioned (Task 1 `existing_non_empty_refused`).
3. Mixed slashes or a trailing backslash (`C:/Program Files/Epic Games/Tide\`): accepted and normalised (Task 1 `slashes_and_trailing_backslash_ok`).
4. The admin copy started with extra or missing arguments: refuses (exit 2), creates nothing (Task 1 `helper_args_exactly_one_path`).
5. Permission given but the folder still isn't writable afterwards: `install_start` refuses with the failed message instead of queuing (Task 2 test on the pure `after_permission(writable: bool)`).

---

### Task 1: Rules and the admin helper (core)

**Files:** Create `crates/fishy-core/src/install/elevate.rs` (`pub mod elevate;` in `install/mod.rs`); modify `crates/fishy-core/Cargo.toml` (windows-sys features `Win32_UI_Shell`, `Win32_UI_WindowsAndMessaging`).

**Interfaces (produces):**
- `pub const ARG: &str = "--make-game-folder";`
- `pub fn check_new_folder(path: &Path, windows_dir: &Path) -> Result<PathBuf, Refused>` (returns the normalised path; `Refused` is a small enum: `NotAbsolute`, `Unsafe`, `DriveRoot`, `NoParent`, `NotEmpty`, `SystemFolder`)
- `pub fn helper_path(args: &[String]) -> Option<PathBuf>` (args after the exe: `[ARG, path]` → Some, else None)
- `pub fn icacls_args(path: &Path) -> Vec<String>`
- `pub enum Asked { Created, Declined, Failed }` and `pub fn asked_from(code: u32) -> Asked` (0 → Created, 1223 → Declined, else Failed)
- `pub fn run_helper(args: &[String]) -> i32` (exit code; uses `check_new_folder`, `create_dir`, icacls; removes the folder on icacls failure)
- `#[cfg(windows)] pub fn ask_for_folder(path: &Path) -> Asked` (ShellExecuteExW runas on `std::env::current_exe()`, `SEE_MASK_NOCLOSEPROCESS`, wait, `GetExitCodeProcess`; `ERROR_CANCELLED` from `GetLastError` → Declined)

- [ ] **Step 1: Failing tests:** `good_path_ok`, `relative_refused`, `dotdot_refused`, `drive_root_refused`, `verbatim_and_network_refused`, `missing_parent_refused`, `existing_non_empty_refused`, `existing_empty_ok`, `inside_windows_refused`, `slashes_and_trailing_backslash_ok`, `helper_args_exactly_one_path`, `icacls_args_grant_users_full_control`, `exit_codes_map`, `helper_refuses_bad_path_with_2_and_creates_nothing`.
- [ ] **Step 2:** `cargo test -p fishy-core elevate` → FAIL. **Step 3:** implement. **Step 4:** tests + clippy → PASS. **Step 5:** commit `feat(admin-folders): path rules and admin helper`.

### Task 2: App wiring

**Files:** `app/src-tauri/src/main.rs` (if `elevate::helper_path(&args[1..])` is Some → `std::process::exit(elevate::run_helper(&args[1..]))` before `run()`; a lone `--make-game-folder` with a bad path also exits 2), `app/src-tauri/src/lib.rs` (`InstallOptions.needs_permission`, `install_options`, `install_start`, `epic_folder` command returning `Option<String>`).

**Interfaces:** `fn after_permission(writable: bool) -> Result<(), InstallError>`; `InstallError::NoWriteAccess` stays for the declined/failed cases, with two new variants or messages: `PermissionDeclined`, `FolderFailed { folder }` (core error.rs, messages from Global Constraints). `epic_folder() -> Option<String>`.

- [ ] **Step 1: Failing tests:** `permission_messages` (error.rs), `after_permission_refuses_when_still_not_writable`, `install_options_needs_permission_instead_of_error` (pure helper building the options' folder fields from `can_create_in` result: `fn folder_fields(res: Result<(), PathBuf>, safe: bool) -> (bool /*folder_ok*/, bool /*needs_permission*/, Option<String>)`).
- [ ] **Step 2–5:** as usual; commit `feat(admin-folders): ask Windows for permission when installing`.

### Task 3: UI

**Files:** `app/src/lib/install/InstallDialog.svelte` (+ test), `app/src/lib/install/installs.ts` (`InstallOptions.needs_permission?`, `epicFolder()`), `app/src/lib/settings/SettingsPage.svelte` (+ test).

- [ ] **Step 1: Failing tests:** `needs_permission_shows_note_and_install_enabled`, `use_epics_folder_asks_again_with_it` (Install box: button calls `install_options` with `parent` = the Epic folder), `use_epics_folder_hidden_without_it`, `declined_message_shown` (install_start error string), Settings `use_epics_folder_saves_it`.
- [ ] **Step 2–5:** as usual; commit `feat(admin-folders): Use Epic's folder and the permission note`.

### Task 4: Review, release 1.2.0, hand check

- [ ] Spec §12 entry + hand checks; README "Your files and PC" line (one permission prompt per game folder in protected places).
- [ ] Fresh whole-branch review (most capable model); fix Critical/Important test-first.
- [ ] 1.2.0: version bump + notes entry, signed build, `latest.json`, Release (draft → public), push `feat/import` and `master`.
- [ ] Hand check with the user: installed app updates to 1.2.0 by itself; install a small game into Epic's folder (one prompt), play, uninstall (no prompt); No in the prompt shows the message.
