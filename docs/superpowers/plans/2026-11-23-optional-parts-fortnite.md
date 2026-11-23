# Optional parts (Fortnite) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Import Fortnite (3 pieces, some optional parts) from Epic's launcher, Play it, update all its pieces with only the parts on disk, uninstall it, and refuse fresh installs of games with optional parts.

**Architecture:** The manifest parser keeps each file's part names. A small pure module `install/parts.rs` decides which files belong (`only_parts` returns a filtered `Manifest`), so the existing plan, engine, space check and file check run unchanged on the filtered manifest. Job and installed records carry `parts` and `parent`; add-on pieces are hidden installed records tied to their game.

**Tech Stack:** Rust (fishy-core, Tauri 2 commands), Svelte 5 + vitest.

**Spec:** `docs/superpowers/specs/2026-11-20-fishy-client-design.md` §6n (with §6c, §6e, §6g).

## Global Constraints

- Logs never contain part-name lists, paths, tokens or the account (spec §12, §6n "Privacy and logs"). Allowed: "epic import: Fortnite with optional parts (N of M part names)", "update: Fortnite piece 2 of 3".
- Nothing ever writes Epic's `.item` files or `.egstore` folder.
- A file with no part name, or only the empty name `""`, always belongs.
- `parts: None` means "every file" and is the default for all existing records and jobs (serde `default`).
- New user-facing text, exactly:
  - Install box: "This game has optional parts. Install it with Epic's launcher; Fishy Client adds it by itself afterwards."
  - ⋯ menu line: "Also in Epic's launcher. Updating in one may make the other update again."
  - Import note: "Imported from Epic's launcher, with the parts Epic installed."
  - Missing piece: "Couldn't import *Title*: a part of it is missing. Open it once in Epic's launcher, then try again."
- Commands in PowerShell use `npm.cmd`. Commits end with the Co-Authored-By line.

## Review Focus

1. Files tagged only `""` (Fortnite has 5): must count as untagged and always belong. Test in Task 2.
2. A file carrying several part names where only one is chosen: belongs. Test in Task 2.
3. `installed.json` / `job.json` written by 1.3.0 (no `parts`, `parent`, `namespace`, `catalog_item_id`): read as None / empty, games keep working. Test in Task 3.
4. Update when the installed manifest is missing or unreadable: use the saved parts only (no "new part" detection), never `None` (that would download every part). Test in Task 2 (`parts_for_update` with `old: None`).
5. A game record whose pieces are already up to date: Update queues only the game; a piece alone outdated still shows Update on the game's card. Tests in Task 6 and Task 8.

---

### Task 1: Manifests keep part names

**Files:**
- Modify: `crates/fishy-core/src/install/manifest/mod.rs` (FileEntry), `manifest/binary.rs:136-140`, `manifest/json.rs:53-66`, `manifest/testbuild.rs:81`, and every `FileEntry { .. }` literal (`compare.rs:34`, `engine.rs` tests, `binary.rs` tests, `testbuild.rs:133-140`).

**Interfaces:**
- Produces: `FileEntry.install_tags: Vec<String>` (empty strings dropped while parsing).

- [ ] **Step 1: Failing tests.** `binary.rs`: `reads_install_tags` builds a manifest with `testbuild` where file 0 has tags `["Langde", ""]` and file 1 none; asserts `files[0].install_tags == ["Langde"]`, `files[1].install_tags.is_empty()`. `json.rs`: `reads_install_tags_from_json` with `"InstallTags": ["GFP_JunoRoot"]` on the file → `["GFP_JunoRoot"]`; missing key → empty.
- [ ] **Step 2:** `cargo test -p fishy-core manifest` → fails (no field).
- [ ] **Step 3:** Add the field; read tags in `binary.rs` (keep non-empty), `json.rs` (`InstallTags` array of strings, optional); `testbuild::build_binary` writes each file's tags.
- [ ] **Step 4:** `cargo test -p fishy-core` → all pass.
- [ ] **Step 5:** Commit `feat(manifest): keep each file's part names (spec §6n)`.

### Task 2: `install/parts.rs`

**Files:** Create `crates/fishy-core/src/install/parts.rs` (+ `pub mod parts;` in `install/mod.rs`).

**Interfaces:**
- Consumes: `FileEntry.install_tags`.
- Produces:
  - `pub fn has_parts(m: &Manifest) -> bool`
  - `pub fn belongs(f: &FileEntry, parts: &[String]) -> bool` — no names, or any name in `parts`.
  - `pub fn chosen_on_disk(root: &Path, m: &Manifest) -> Option<Vec<String>>` — `None` when `!has_parts(m)`; else every part name with at least one of its files present (`file_path` + `is_file`), sorted, unique.
  - `pub fn only_parts(m: Manifest, parts: Option<&[String]>) -> Manifest` — `None` returns `m` unchanged; else keeps only belonging files (chunks list unchanged).
  - `pub fn parts_for_update(saved: &[String], old: Option<&Manifest>, new: &Manifest) -> Vec<String>` — saved plus every name in `new` not in `old`; `old: None` → `saved` only. Sorted, unique.

- [ ] **Step 1: Failing tests** (manifests from `testbuild`/`Manifest::new`): `untagged_and_empty_named_files_always_belong`; `a_file_belongs_if_any_of_its_names_is_chosen` (`["A","B"]` with chosen `["B"]` → true; with `["C"]` → false); `chosen_on_disk_reads_the_files` (tempdir with files of part A present, B missing → `Some(["A"])`; manifest without names → `None`); `only_parts_none_keeps_everything` and `only_parts_drops_unchosen_files`; `update_adds_brand_new_parts` (saved `["A"]`, old has A,B, new has A,B,C → `["A","C"]`); `update_without_old_manifest_keeps_saved_parts` (`old: None` → `["A"]`).
- [ ] **Step 2:** `cargo test -p fishy-core parts` → fails (no module).
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `cargo test -p fishy-core` → pass.
- [ ] **Step 5:** Commit `feat(install): which files belong to the chosen parts (spec §6n)`.

### Task 3: Records carry parts and parent

**Files:** Modify `crates/fishy-core/src/install/store.rs` (`JobInfo`, `InstalledGame`, `finish_install`), all `JobInfo { .. }` / `InstalledGame { .. }` literals (`epic_import.rs`, `uninstall.rs:99`, `wipe.rs:130`, `engine.rs` tests, `client.rs` tests, `app/src-tauri/src/{downloads,lib,updates,epic_import}.rs`).

**Interfaces:**
- Produces: `JobInfo.parts: Option<Vec<String>>`, `JobInfo.parent: Option<String>`; `InstalledGame.parts: Option<Vec<String>>`, `InstalledGame.parent: Option<String>`, `InstalledGame.namespace: String`, `InstalledGame.catalog_item_id: String` (all `#[serde(default)]`). `finish_install` copies the four from the job.

- [ ] **Step 1: Failing tests** in `store.rs`: `records_from_1_3_0_read_without_parts` (old JSON for job and installed → `parts == None`, `parent == None`, namespace `""`); `finish_install_keeps_parts_and_parent` (job with `parts: Some(["A"])`, `parent: Some("Fortnite")`, namespace `"ns"` → record has the same).
- [ ] **Step 2:** `cargo test --workspace` → fails to compile.
- [ ] **Step 3:** Add fields; fix literals with `parts: None, parent: None` (and empty namespace/catalog in tests).
- [ ] **Step 4:** `cargo test --workspace` → pass.
- [ ] **Step 5:** Commit `feat(store): installs remember their parts and parent game (spec §6n)`.

### Task 4: Epic's list keeps add-on pieces

**Files:** Modify `crates/fishy-core/src/epic_import.rs` (`EpicItem`, `read_item`, `import_job`, tests `optional_parts_marked_partial` → replaced).

**Interfaces:**
- Produces: `EpicItem.parent: Option<String>` replaces `partial` (`Some(MainGameAppName)` for an add-on in the **same install folder**; add-ons elsewhere are still skipped). `pub fn pieces_of<'a>(items: &'a [EpicItem], game: &EpicItem) -> Vec<&'a EpicItem>` (parent == game.app_name and same `install_path`, sorted by app name). `import_job(item, title, m, base_urls, parts: Option<Vec<String>>) -> JobInfo` sets `parts`, `parent: item.parent.clone()`, `namespace`, `catalog_item_id`.

- [ ] **Step 1: Failing tests:** `scan_keeps_add_ons_with_their_game` (game + add-on with `MainGameAppName` in the same folder → add-on has `parent == Some("Fortnite")`; add-on in another folder → not listed); `pieces_of_finds_the_add_ons`; `import_job_carries_parts_and_parent`.
- [ ] **Step 2:** `cargo test -p fishy-core epic_import` → fails.
- [ ] **Step 3:** Implement. Remove `partial`.
- [ ] **Step 4:** `cargo test -p fishy-core` → pass (app crate fixed in Task 5).
- [ ] **Step 5:** Commit together with Task 5.

### Task 5: Import Fortnite with its pieces and parts

**Files:** Modify `app/src-tauri/src/epic_import.rs`.

**Interfaces:**
- Consumes: `pieces_of`, `chosen_on_disk`, `only_parts`, `import_job(.., parts)`.
- `candidates` skips items with `parent.is_some()` (instead of `partial`).
- `manifest_for(item: &EpicItem, auth, install, need_info)` uses the item's namespace / catalog item / app name (pieces aren't Library games).
- `epic_import_add`: for the game then each piece (`pieces_of(scan)`): manifest (`.egstore` copy; a piece with none → error "Couldn't import *Title*: a part of it is missing. Open it once in Epic's launcher, then try again."), `parts = chosen_on_disk`, `m = only_parts(m, parts)`, `check`. Any bad file in any piece → `AddOutcome::Missing` with summed counts. All good → `add_installed` for **pieces first, then the game**. Piece title = the item's Epic `title`. Log "epic import: {app} with optional parts ({n} of {m} part names)" when parts is Some.
- `epic_import_repair`: one repair job per piece (and game) with bad files, each with its `parts`.

- [ ] **Step 1: Failing test:** `candidates_skip_add_on_pieces` (replaces `candidates_skip_partial_installs`): an item with `parent: Some("Fortnite")` is not a candidate; Fortnite itself is.
- [ ] **Step 2:** `cargo test --workspace` → fails.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `cargo test --workspace` and `cargo clippy --workspace --all-targets` → pass, no warnings.
- [ ] **Step 5:** Commit `feat(import): Fortnite with its pieces and the parts on disk (spec §6n)`.

### Task 6: Updates, resume and uninstall use parts and pieces

**Files:** Modify `app/src-tauri/src/updates.rs`, `app/src-tauri/src/downloads.rs:241-258`, `app/src-tauri/src/lib.rs` (`uninstall_game`).

**Interfaces:**
- `pub(crate) fn pieces_to_update(game: &str, installed: &[InstalledGame], latest: &HashMap<String, String>) -> Vec<String>` in `updates.rs`: the game (if outdated) then its pieces (records with `parent == Some(game)`) whose latest differs, by app name.
- `queue_update` becomes a loop over `pieces_to_update` (with the game's library entry for the game, the record's namespace / catalog item for pieces). Each piece: new manifest → `parts = record.parts.map(|p| parts_for_update(&p, old.as_ref(), &new))`; plan, space check and job use `only_parts(new, parts)` and `only_parts(old, record.parts)`; job gets `parts`, `parent`. A game whose own version is current but a piece is outdated still queues the piece. Log "update: {game} piece {i} of {n}".
- `Downloads::new` (resume after restart): filter the job manifest by `job.parts` and the old one by the installed record's `parts`.
- `uninstall_game`: uninstall pieces (`parent == Some(app)`) first, then the game.

- [ ] **Step 1: Failing tests** in `updates.rs`: `only_outdated_pieces_are_updated` (game current, STW outdated, LEGO current → `["aa31…"]`); `game_first_then_pieces` (all outdated → game, then pieces sorted); `pieces_of_other_games_are_ignored`.
- [ ] **Step 2:** `cargo test --workspace` → fails.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `cargo test --workspace`, clippy → pass.
- [ ] **Step 5:** Commit `feat(updates): update a game's pieces with their parts (spec §6n)`.

### Task 7: Install box refuses games with optional parts

**Files:** Modify `app/src-tauri/src/lib.rs` (`InstallOptions`, `install_options`, `install_start`), `app/src/lib/install/installs.ts` (`InstallOptions` type), `app/src/lib/install/InstallDialog.svelte`, `InstallDialog.test.ts`, `app/src/lib/auth/devMock.ts`.

**Interfaces:** `InstallOptions.has_parts: bool` (`parts::has_parts(&p.manifest)`). `install_start` refuses with the Install-box sentence when the manifest has parts.

- [ ] **Step 1: Failing vitest:** `optional_parts_are_installed_with_epic`: options with `has_parts: true` show "This game has optional parts. Install it with Epic's launcher; Fishy Client adds it by itself afterwards." and no Install button.
- [ ] **Step 2:** `npm.cmd --prefix app test` → fails.
- [ ] **Step 3:** Implement (Rust field + guard, TS type, dialog).
- [ ] **Step 4:** `cargo test --workspace`, `npm.cmd --prefix app test`, `npm.cmd --prefix app run check` → pass.
- [ ] **Step 5:** Commit `feat(install): games with optional parts install through Epic's launcher (spec §6n)`.

### Task 8: Library shows pieces' updates and the Epic note

**Files:** Modify `app/src/lib/install/installs.ts` (`InstalledGame` gains `parts?: string[] | null`, `parent?: string | null`; `hasUpdate`; `autoUpdate`), `app/src/lib/library/LibraryPage.svelte` (`jobOf`, ⋯ menu line), `app/src/lib/import/EpicImportNote.svelte` (note text for games with parts, if it names the game), tests in `installs.test.ts` / `library.test.ts`.

**Interfaces:**
- `hasUpdate(appName, ins, latest)`: true if the game **or any record with `parent === appName`** has a differing latest version.
- `autoUpdate` skips records with a `parent`.
- `jobOf(g)`: a download of the game or of any of its pieces.
- ⋯ menu: when the installed record has `parts` (not null), the line "Also in Epic's launcher. Updating in one may make the other update again."

- [ ] **Step 1: Failing tests:** `update_shown_when_only_a_piece_is_outdated`; `auto_update_skips_pieces`; `menu_says_also_in_epic_for_games_with_parts`; `a_piece_download_shows_updating_on_the_game`.
- [ ] **Step 2:** `npm.cmd --prefix app test` → fails.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `npm.cmd --prefix app test`, `npm.cmd --prefix app run check` → pass.
- [ ] **Step 5:** Commit `feat(library): Fortnite's pieces in updates, downloads and the menu (spec §6n)`.

### Task 9: Docs, review, release 1.4.0

- [ ] README: Fortnite and games with optional parts (import from Epic, update in Fishy, fresh install in Epic's launcher). Progress log in the spec.
- [ ] Fresh whole-branch review on the most capable model; fix findings.
- [ ] Release 1.4.0 per `docs/RELEASING.md` (notes in `release-notes.json`, version lines, `notes.test.ts`), push `feat/import` and `master`.
- [ ] Hand check with the user (spec §6n "By hand").
