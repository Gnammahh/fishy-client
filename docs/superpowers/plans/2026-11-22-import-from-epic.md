# Import from Epic's launcher Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Games already installed by Epic's launcher can be added to Fishy Client as installed, with only missing or damaged files downloaded.

**Architecture:** A new core module `fishy_core::epic_import` reads Epic's `.item` files and each game's `.egstore` manifest (read-only), checks file sizes, and either finishes the game as an install (existing `InstallStore::finish_install`) or saves a download job whose `resume.txt` already lists the good files, so the existing engine downloads only the rest. Three Tauri commands in a new `app/src-tauri/src/epic_import.rs`; one Svelte note component at the top of the Library.

**Tech Stack:** Rust (serde_json, tempfile, wiremock), Tauri 2, Svelte 5, vitest.

**Spec:** `docs/superpowers/specs/2026-11-20-fishy-client-design.md` §6g (also §6c for stores/jobs, §12 for privacy).

## Global Constraints

- Never write, move or delete anything in Epic's `Manifests` folder or in `<InstallLocation>\.egstore`.
- Epic folder: `%PROGRAMDATA%\Epic\EpicGamesLauncher\Data\Manifests\`, files `*.item`.
- `.egstore` manifest: `<InstallLocation>\.egstore\<InstallationGuid>.manifest`.
- Skip: `bIsIncompleteInstall == true`; `MainGameAppName` non-empty and different from `AppName` (add-on); `InstallLocation` not an existing folder; unreadable/invalid JSON.
- Check = size only (no SHA-1). File names through `store::file_path` (unsafe name counts as bad).
- Logs: counts and app names only; never folder paths or file names.
- Copy (exact): note `"{n} games installed by Epic's launcher can be added."` (`"1 game installed by Epic's launcher can be added."` for one), button `Add them`, `"Checking {title}…"`, `"{title}: {n} files are missing or damaged ({size})."` (`1 file is`), button `Download them`, `"Couldn't read {title}'s files from Epic's launcher. Press Install to download it."`, `"Couldn't reach Epic to check {title}. Check your internet."`.
- PowerShell: use `npm.cmd`. Commit messages end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. A game already installed in Fishy Client, or with a download queued, must not be offered or added again (Task 4 test `candidates_skip_installed_and_queued`).
2. Cancel of an imported repair must not delete the game files that were already there (Task 1 test `cancel_of_imported_job_keeps_game_files`).
3. An `.egstore` manifest for a different app (copied folder) must not be used: `meta.app_name` must equal the item's `AppName`, else fall back to downloading (Task 3 test `manifest_for_other_app_is_not_used`).
4. Fortnite's add-on `.item`s share the main game's folder; they must be skipped, and the main game kept (Task 2 test `addons_skipped_main_kept`).
5. `.item` files are UTF-8, sometimes with a BOM; titles like "Rocket League®" must still read (Task 2 test `reads_item_with_bom`).

---

### Task 1: `JobInfo.imported` and Cancel keeping game files

**Files:**
- Modify: `crates/fishy-core/src/install/store.rs` (`JobInfo`)
- Modify: `crates/fishy-core/src/install/uninstall.rs` (`discard_download`)
- Fix every `JobInfo { .. }` literal (`app/src-tauri/src/lib.rs` `install_start`, `app/src-tauri/src/updates.rs`, tests) with `imported: false`.

**Interfaces:**
- Produces: `JobInfo.imported: bool` (`#[serde(default)]`).

- [ ] **Step 1: Failing tests**
  - `store.rs`: `old_job_json_without_imported_reads_as_false` (JSON without the field → `imported == false`).
  - `uninstall.rs`: `cancel_of_imported_job_keeps_game_files`: save a job with `imported: true` whose manifest lists `a.bin`, create `a.bin` in the install folder, call `discard_download` → `a.bin` still exists, download folder gone.
- [ ] **Step 2:** `cargo test -p fishy-core imported` → FAIL (no field).
- [ ] **Step 3:** Add the field; in `discard_download`, skip `delete_manifest_files` when `job.imported`.
- [ ] **Step 4:** `cargo test -p fishy-core` and `cargo check -p fishy-client` (from repo root) → PASS / compiles.
- [ ] **Step 5:** Commit `feat(import): job.json imported flag; Cancel keeps an imported game's files`.

### Task 2: Reading Epic's `.item` files

**Files:**
- Create: `crates/fishy-core/src/epic_import.rs` (add `pub mod epic_import;` in `crates/fishy-core/src/lib.rs`)

**Interfaces:**
- Produces:
  - `pub struct EpicItem { pub app_name: String, pub title: String, pub install_path: PathBuf, pub installation_guid: String, pub version: String, pub namespace: String, pub catalog_item_id: String }` (Debug, Clone, PartialEq)
  - `pub fn epic_manifests_dir() -> Option<PathBuf>`: `%PROGRAMDATA%` joined with `Epic\EpicGamesLauncher\Data\Manifests`; None if the env var is missing.
  - `pub fn scan(dir: &Path) -> Vec<EpicItem>`: every `*.item`, skipped per Global Constraints, sorted by lowercase title, deduplicated by `app_name`. Missing dir → empty.
  - `pub fn local_manifest(item: &EpicItem) -> Option<Vec<u8>>`: reads the `.egstore` file; None if missing.

- [ ] **Step 1: Failing tests** (tempfile; a helper `write_item(dir, guid, json)` and a real-shaped item JSON with the PascalCase keys from Global Constraints):
  - `reads_a_game` (all 7 fields mapped: `AppName`, `DisplayName`, `InstallLocation`, `InstallationGuid`, `AppVersionString`, `CatalogNamespace`, `CatalogItemId`)
  - `addons_skipped_main_kept` (two items, add-on has `MainGameAppName: "Fortnite"`, main has `""`)
  - `incomplete_install_skipped`, `missing_folder_skipped`, `broken_json_and_other_files_skipped`
  - `reads_item_with_bom` (file starts with `\u{FEFF}`, title `"Rocket League®"`)
  - `missing_dir_is_empty`
  - `local_manifest_found_and_missing`
- [ ] **Step 2:** `cargo test -p fishy-core epic_import` → FAIL.
- [ ] **Step 3:** Implement with a private serde struct (`#[serde(rename_all = "PascalCase")]`, `bIsIncompleteInstall` renamed explicitly, all fields `#[serde(default)]`); strip a leading BOM before parsing.
- [ ] **Step 4:** Run → PASS.
- [ ] **Step 5:** Commit `feat(import): read Epic's installed-games list`.

### Task 3: Checking files and adding / repairing

**Files:**
- Modify: `crates/fishy-core/src/epic_import.rs`
- Test (repair through the engine): `crates/fishy-core/src/install/engine.rs` tests module (reuse `serve`/`env`/`Fixture`)

**Interfaces:**
- Consumes: `EpicItem` (Task 2), `JobInfo.imported` (Task 1), `InstallStore::{save_job, mark_finished, finish_install}`, `parse_manifest`, `store::file_path`.
- Produces:
  - `pub struct FileCheck { pub good: Vec<(String, String)> /* (sha1 hex, name) */, pub bad_files: usize, pub bad_bytes: u64 }`, `impl FileCheck { pub fn all_good(&self) -> bool }`
  - `pub fn check_files(root: &Path, m: &Manifest) -> FileCheck` (size from `std::fs::metadata`; any error or wrong size or unsafe name = bad; `bad_bytes` = sum of manifest sizes of bad files)
  - `pub fn usable_manifest(item: &EpicItem, bytes: &[u8]) -> Option<Manifest>`: parses; None if parse fails or `meta.app_name != item.app_name`.
  - `pub fn import_job(item: &EpicItem, title: &str, m: &Manifest, base_urls: Vec<String>) -> JobInfo` (`imported: true`, `update_from: None`, `download_size` = sum of chunk `file_size`, `install_size` = `m.install_size()`)
  - `pub fn add_installed(store: &InstallStore, job: &JobInfo, bytes: &[u8], m: &Manifest) -> io::Result<InstalledGame>`: `save_job` then `finish_install`.
  - `pub fn save_repair(store: &InstallStore, job: &JobInfo, bytes: &[u8], check: &FileCheck) -> io::Result<()>`: `save_job` then `mark_finished` for each good file.

- [ ] **Step 1: Failing tests** (manifests from `install::manifest::testbuild`):
  - `all_files_present_is_all_good`, `missing_file_is_bad`, `wrong_size_is_bad` (checks `bad_files` and `bad_bytes`), `unsafe_name_is_bad`
  - `manifest_for_other_app_is_not_used`, `bad_manifest_bytes_not_used`
  - `add_installed_writes_record_and_manifest`: record has manifest's `build_version`, `launch_exe`, `launch_command`, the item's folder; `installed_manifest(app)` is Ok; `pending_jobs()` empty.
  - `save_repair_lists_only_good_files`: `finished_names` equals the good names; `pending_jobs()[0].imported`.
  - engine.rs: `imported_repair_downloads_only_missing_files`: write one of the fixture's files correctly, `save_repair`, `run_job` → only the missing file's chunks are requested (`serve` with `expect`), all files correct after.
- [ ] **Step 2:** `cargo test -p fishy-core` → new tests FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run → PASS; `cargo clippy -p fishy-core --all-targets -- -D warnings` clean.
- [ ] **Step 5:** Commit `feat(import): check an Epic game's files, add it or save a repair`.

### Task 4: Tauri commands

**Files:**
- Create: `app/src-tauri/src/epic_import.rs`; register in `app/src-tauri/src/lib.rs` `invoke_handler`.
- `lib.rs`: make what the commands need reachable (`AppInstall.downloads`, `AppLibrary.cache`, `prepare`, `session::expire`) with `pub(crate)` where needed.

**Interfaces:**
- Consumes: Task 2/3 functions; `Downloads::{enqueue, has_job, view, store, client}`; `LibraryCache` load; `AppAuth.manager.access_token()`; `InstallClient::{manifest_info, download_manifest}`.
- Produces (JSON as the UI sees it):
  - `epic_import_scan() -> Vec<{ app_name: string, title: string }>` (title from the Library's `Game.title`)
  - `epic_import_add(app_name) -> { kind: "added" } | { kind: "missing", files: number, bytes: number } | { kind: "error", message: string, signed_out: boolean }` (serde `tag = "kind"`, lowercase)
  - `epic_import_repair(app_name) -> Result<DownloadsView, String>`
  - pure helper `fn candidates(items: Vec<EpicItem>, games: &[Game], installed: &[InstalledGame], has_job: impl Fn(&str) -> bool) -> Vec<(EpicItem, Game)>`

- [ ] **Step 1: Failing tests** in `epic_import.rs`: `candidates_keep_only_library_games`, `candidates_skip_installed_and_queued`.
- [ ] **Step 2:** `cargo test -p fishy-client candidates` → FAIL.
- [ ] **Step 3:** Implement:
  - scan: `epic_manifests_dir()` → `scan` → `candidates`; `info!("epic import: {} found", n)`.
  - add: re-run `candidates` for that app (none → error "Refresh your Library, then try again."); manifest = `usable_manifest(local_manifest)`, else download it via the token + `manifest_info` + `download_manifest` (network error → "Couldn't reach Epic to check {title}. Check your internet."; expired session → `session::expire` and `signed_out: true`; unusable → "Couldn't read {title}'s files…"). Run `check_files` in `tokio::task::spawn_blocking`. All good → `add_installed` with empty `base_urls` and refresh nothing else (UI reloads installed); `info!("epic import: added {app}")`. Else → `missing` with counts; `info!("epic import: {app} has {n} files to download")`.
  - repair: same manifest step, plus `manifest_info` for `base_urls` (always needed here), `check_files` again, `import_job`, `save_repair`, `downloads.enqueue(&app, job, Arc::new(m), None)`.
- [ ] **Step 4:** `cargo test -p fishy-client` and `cargo clippy --workspace --all-targets -- -D warnings` → PASS/clean.
- [ ] **Step 5:** Commit `feat(import): Tauri commands to scan, add and repair Epic games`.

### Task 5: Library note

**Files:**
- Create: `app/src/lib/import/epicImport.ts`, `app/src/lib/import/EpicImportNote.svelte`, `app/src/lib/import/EpicImportNote.test.ts`
- Modify: `app/src/lib/library/LibraryPage.svelte` (render `<EpicImportNote />` above the tabs, only when signed in and the Library has loaded; reuse its `.banner` look)

**Interfaces:**
- Consumes: Task 4 commands; `refreshInstalled()` and `downloads` from `$lib/install/installs`; `formatBytes`-equivalent already used in the UI (search `format` in `app/src/lib/install`).
- Produces: `scanEpic(): Promise<Candidate[]>`, `addFromEpic(appName): Promise<AddResult>`, `repairFromEpic(appName): Promise<string | null>` (error message or null).

- [ ] **Step 1: Failing tests** (`mockIPC`, as in `installs.test.ts`):
  - shows "3 games installed by Epic's launcher can be added." and singular for 1; nothing when 0 or the scan fails
  - × hides it
  - Add them calls `epic_import_add` once per game in order, shows "Checking {title}…", then calls `installed_list` (refresh)
  - a `missing` result shows "{title}: 12 files are missing or damaged (340 MB)." and Download them calls `epic_import_repair`
  - an `error` result shows its message
- [ ] **Step 2:** `npm.cmd test -- EpicImportNote` (from `app/`) → FAIL.
- [ ] **Step 3:** Implement. The note stays while any result line is shown; it disappears when all games were added.
- [ ] **Step 4:** `npm.cmd test` and `npm.cmd run check` → PASS/clean.
- [ ] **Step 5:** Commit `feat(import): Library note to add games from Epic's launcher`.

### Task 5b: Game size in the "⋯" menu (spec §6g.2)

**Files:**
- Modify: `app/src/lib/library/LibraryPage.svelte` (the `.menu` at the `menuFor` block)
- Test: `app/src/lib/library/library.test.ts`

**Interfaces:**
- Consumes: `formatBytes` from `$lib/progress/progress`; `InstalledGame.install_size` from `$installed`.

- [ ] **Step 1: Failing test** `menu_shows_installed_size`: installed record with `install_size: 13207024435`, open "⋯" → text `Size: ${formatBytes(13207024435)}` is shown above Uninstall.
- [ ] **Step 2:** `npm.cmd test -- library` → FAIL.
- [ ] **Step 3:** Add `<p class="size" role="none">Size: {formatBytes(size)}</p>` as the menu's first line (not a menuitem).
- [ ] **Step 4:** `npm.cmd test` and `npm.cmd run check` → PASS.
- [ ] **Step 5:** Commit `feat: game size in the ⋯ menu`.

### Task 5c: Folder check in the Install box (spec §6g.3)

**Files:**
- Modify: `crates/fishy-core/src/install/space.rs` (new function + tests)
- Modify: `crates/fishy-core/src/install/error.rs` (new variant + `user_message`)
- Modify: `app/src-tauri/src/lib.rs` (`InstallOptions`, `install_options`, `install_start`)
- Modify: `app/src/lib/install/InstallDialog.svelte`, `app/src/lib/install/installs.ts` (`InstallOptions` type), `app/src/lib/install/InstallDialog.test.ts`

**Interfaces:**
- Produces:
  - `pub fn can_create_in(path: &Path) -> Result<(), PathBuf>`: walks up from `path` to the nearest existing folder, creates and removes `.fishy-write-test` there; `Err(that_folder)` if creating fails.
  - `InstallError::NoWriteAccess { folder: String }`, message exactly `"Fishy Client can't create folders in {folder} without administrator rights. Pick a different folder."`
  - `InstallOptions.folder_ok: bool`; when false, `error` holds the message above (it wins over the space message).

- [ ] **Step 1: Failing tests**
  - space.rs: `can_create_in_temp_folder`, `can_create_in_missing_child_checks_parent`, `cannot_create_under_a_file` (path under an existing file → `Err`; a read-only folder attribute doesn't block creating on Windows, so this is the portable failing case), `test_folder_is_removed`.
  - error.rs: `no_write_access_message` with `C:\Program Files\Epic Games`.
  - InstallDialog.test.ts: `folder_not_ok_disables_install_and_shows_message`.
- [ ] **Step 2:** `cargo test -p fishy-core can_create` and `npm.cmd test -- InstallDialog` → FAIL.
- [ ] **Step 3:** Implement. `install_options`: call `can_create_in(&path)` in `spawn_blocking`; set `folder_ok` and `error`. `install_start`: same check before `check_space`, refuse with the message. `InstallDialog`: `disabled={!options.enough_space || !options.folder_ok || busy}`.
- [ ] **Step 4:** `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `npm.cmd test`, `npm.cmd run check` → all PASS/clean.
- [ ] **Step 5:** Commit `feat: Install box says when a folder needs admin rights`.

### Task 6: Docs, review, push, hand check

- [ ] README: "What is stored / read" — add a row or line: Fishy Client **reads** (never changes) `C:\ProgramData\Epic\EpicGamesLauncher\Data\Manifests\` and each Epic game's `.egstore` folder, only when the Library opens, to offer **Add them**. Update "Your files" sentence accordingly. Add a short "Games from Epic's launcher" usage note with the shared-folder limits.
- [ ] Spec §12: progress-log entry (version 1.1 import), new test counts, privacy line, hand-check list: note appears with the right count; Add them on a game with all files → Installed and Play starts; rename one small game file → missing line, Download them fetches only it; Cancel keeps files; Epic's launcher still sees the game; the ⋯ menu shows the size; picking `C:\Program Files\Epic Games` in the Install box shows the admin message and greys out Install.
- [ ] Fresh whole-branch review (superpowers:requesting-code-review), fix findings test-first.
- [ ] `git push -u origin feat/import` (only this branch).
- [ ] Start `npm.cmd run tauri dev` in `app/` in a terminal tab for the user's hand check.
