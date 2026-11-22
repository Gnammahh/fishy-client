# Polish 2: Friendlier Errors Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every error tells the user what to do next. Brief connection problems with Epic are retried automatically. An expired sign-in sends the user back to Login without losing anything. A screen bug shows a calm crash page instead of a blank window.

**Architecture:**
- **fishy-core gains:**
  - a small `retry` module (a `Transient` trait and `with_retries`), wrapped around the named Epic calls
  - an `AuthError::Expired` that `AuthManager::session()` returns when Epic refuses the saved sign-in
  - `InstallError::FilesInUse` for Windows sharing and lock violations
- **The app gains:** one `expire()` path (pause downloads, forget the sign-in and the saved library, tell the UI), used wherever a token is fetched, plus a `log_ui_error` command.
- **The UI gains:** a crash boundary around the whole layout, global error logging, and clearing the library whenever the sign-in ends.

**Tech Stack:** Rust (reqwest, wiremock, tempfile, tokio), Tauri 2, Svelte 5 (`<svelte:boundary>`), vitest.

**Spec:** `docs/superpowers/specs/2026-11-20-fishy-client-design.md` §6f.2 (and §7, §12 "Privacy and security decisions")

## Global Constraints
- **The UI text is exactly as in the §6f.2 table**, plus:
  - "Some of *Title*'s files are in use. Close the game, then press Retry."
  - "Your sign-in has expired. Please sign in again."
  - "Something went wrong"
  - "Fishy Client hit a problem it didn't expect. Your games and downloads are safe."
  - the buttons "Reload" and "Open logs folder"
- **Retries:** at most 3 tries, waiting `base` then `2 × base`, with `base` = 500 ms in the shipped app.
  - **Transient:** `AuthError::Network`, `InstallError::Network`, and any HTTP status from 500 to 599.
  - **Never retried:** `EpicClient::exchange_code` (the one-time sign-in code) and `kill_session` (Log out). Game chunks keep their own existing retries.
- **Expired** means `refresh` came back `Rejected`, or there's no saved session. `Network` and `Unexpected` are never expired.
- **On expiry:** downloads pause (they're never cancelled or taken off the queue), the Credential Manager entry and the saved library are deleted, and settings and installed games stay. Epic's kill-session isn't called.
- **Logs:** retries log `retrying <what> (<n> of 3)` with no URLs or tokens. UI errors are logged as `ui error: <name>: <message>`, cut to 500 characters, and go through the existing scrubber.
- Run UI commands as `npm.cmd` from `app/`.
- **Tests stay fast:** any existing test that now hits a retried failure adds `.with_retry_base(Duration::from_millis(1))` to its client.

## Review Focus
- **Epic answers 503 while refreshing the sign-in:** you stay signed in (Offline), never "expired". Tested in Task 1 (5xx becomes Network) and Task 3.
- **The sign-in expires while a download is running:** the download is paused and kept, and after signing in again Resume continues it. Tested in Task 4 (`expire` only pauses).
- **One of the 164 game-detail lookups fails twice, then works:** that game still gets its details, and Refresh doesn't fail. Tested in Task 2.
- **The crash screen while logging is unavailable:** if `log_ui_error` itself fails, the crash page still shows and nothing throws again. Tested in Task 6.
- **The old account's games:** after an expiry, none stay in memory or on the Library screen. Tested in Task 4 (the UI clears the library on a signed-out event).

---

### Task 1: Retry helper; Epic 5xx counts as a connection problem

**Files:**
- Create: `crates/fishy-core/src/retry.rs`
- Modify: `crates/fishy-core/src/lib.rs` (`pub mod retry;`), `crates/fishy-core/src/auth/client.rs` (`read_json`)

**Interfaces:**
- Produces:
  - `pub const RETRY_BASE: Duration` (500 ms)
  - `pub const TRIES: u32` (3)
  - `pub trait Transient { fn is_transient(&self) -> bool; }`, implemented for `AuthError` (`Network(_)` only) and `InstallError` (`Network(_)` only)
  - `pub async fn with_retries<T, E: Transient, Fut: Future<Output = Result<T, E>>>(what: &str, base: Duration, f: impl FnMut() -> Fut) -> Result<T, E>`

- [ ] **Step 1: Write the failing tests.** Use `#[tokio::test(start_paused = true)]` with a counter closure.
  - `succeeds_after_two_transient_failures`: Network, Network, then Ok(7) gives `Ok(7)`, 3 calls, and about 1.5 s elapsed with base 500 ms.
  - `gives_up_after_three_tries`: always Network gives `Err(Network)` and exactly 3 calls.
  - `a_real_answer_is_not_retried`: `Rejected { code: "x" }` gives `Err` and 1 call.
  - In `auth/client.rs`:
    - `server_errors_count_as_network`: wiremock returns 503 with body `{"errorCode":"errors.com.epicgames.common.server_error"}`. `read_json` gives `AuthError::Network(_)`.
    - 401 with an errorCode still gives `Rejected`.
- [ ] **Step 2: Run them and see them fail.** Run `cargo test -p fishy-core retry:: auth::client`. Expected: FAIL.
- [ ] **Step 3: Implement.**
  - `read_json`: any status of 500 or above becomes `AuthError::Network(format!("http_{status}"))` before the errorCode check. Epic's text is still never kept.
  - `with_retries` sleeps `base × 2^(attempt-1)` before attempts 2 and 3, and logs `tracing::info!("retrying {what} ({n} of 3)")`.
- [ ] **Step 4: Run the tests again.** Run `cargo test -p fishy-core`. Expected: PASS. Fix any existing test that relied on 5xx being `Rejected` by updating its expected value. The manifest_info `500..` arm already treats it as Network.
- [ ] **Step 5: Commit.** `feat(core): retry helper; Epic server errors count as connection problems`

### Task 2: Retries on the Epic calls

**Files:**
- Modify: `crates/fishy-core/src/library/client.rs`, `crates/fishy-core/src/auth/client.rs`, `crates/fishy-core/src/install/client.rs`, `crates/fishy-core/src/launch/client.rs`

**Interfaces:**
- Consumes: `with_retries`, `RETRY_BASE`
- Produces:
  - Each of `LibraryClient`, `EpicClient` and `LaunchClient` gains a `retry_base: Duration` field, `RETRY_BASE` by default, and `pub fn with_retry_base(self, d: Duration) -> Self`.
  - `InstallClient::with_retry_base` now sets both its existing chunk base and the new API base.

- [ ] **Step 1: Write the failing wiremock tests.** In each, a mock answers 503 twice, then success (`up_to_n_times(2)` on the failing mock, mounted first). Each client uses `.with_retry_base(Duration::from_millis(1))`.
  - `library::client`:
    - `items_retry_after_server_errors`
    - `windows_assets_retry_after_server_errors`
    - `a_catalog_lookup_that_fails_twice_still_fills_in_the_game`, through `fetch_library`: the game has its title from the catalog
  - `auth::client`:
    - `refresh_retries_after_server_errors`
    - `exchange_code_is_tried_once`: 503 always, expect exactly 1 request (`.expect(1)`)
  - `install::client`:
    - `manifest_info_retries_after_server_errors`
    - `download_manifest_retries_when_every_server_fails_then_recovers`: 503 twice on the only URL, then the bytes
  - `launch::client`: `exchange_code_and_ownership_token_retry_after_server_errors`
  - Everywhere: a 404 answer is requested exactly once.
- [ ] **Step 2: Run them and see them fail.** Run `cargo test -p fishy-core retry`. Expected: FAIL (one request only, then an error).
- [ ] **Step 3: Implement.** Wrap each named call's request-and-read in `with_retries("<what>", self.retry_base, || async { … })`. The `what` labels are:
  - "library items"
  - "Windows downloads list"
  - "game details"
  - "sign-in refresh"
  - "download info"
  - "file list"
  - "launch code"
  - "ownership token"

  In `items`, wrap each page request, not the whole loop.
- [ ] **Step 4: Run the tests again.** Run `cargo test -p fishy-core`. Expected: PASS, with no test slower than before (add `with_retry_base(1 ms)` where needed).
- [ ] **Step 5: Commit.** `feat(core): retry Epic calls up to 3 times on connection problems`

### Task 3: Expired sign-in in the core

**Files:**
- Modify: `crates/fishy-core/src/auth/error.rs`, `crates/fishy-core/src/auth/manager.rs`, `crates/fishy-core/src/install/error.rs`, `crates/fishy-core/src/launch/error.rs`, `app/src-tauri/src/play.rs` (`session_error`)

**Interfaces:**
- Produces:
  - `AuthError::Expired`, whose display text is "Your sign-in has expired. Please sign in again."
  - `InstallError::Expired`, with the same text from `user_message()`; `From<AuthError>` maps `Expired` to `Expired`
  - `LaunchError` maps `AuthError::Expired` to `SignedOut`
  - `AuthManager::session()`:
    - no saved session gives `Err(Expired)`
    - `refresh` giving `Rejected` clears the store, then gives `Err(Expired)`
    - `Network` and `Unexpected` pass through and keep the store

- [ ] **Step 1: Write the failing tests** in `auth/manager.rs`, using the existing fake store and wiremock helpers:
  - `a_refused_refresh_is_expired_and_forgets_the_sign_in`: the token is near expiry, refresh answers 400 `invalid_grant`. `access_token()` gives `Err(Expired)`, and the store is empty.
  - `a_refresh_epic_cant_answer_keeps_the_sign_in`: refresh answers 503 three times. The result is `Err(Network(_))`, and the store still has the session.
  - `no_saved_sign_in_is_expired`
  - In `install/error.rs`: `InstallError::from(AuthError::Expired) == InstallError::Expired`, and its `user_message()` is the exact text.
- [ ] **Step 2: Run them and see them fail.** Run `cargo test -p fishy-core auth::manager install::error`. Expected: FAIL.
- [ ] **Step 3: Implement.** Update `play.rs::session_error` so `Expired` (and `Rejected`) map to `LaunchError::SignedOut`.
- [ ] **Step 4: Run the tests again.** Run `cargo test --workspace`. Expected: PASS.
- [ ] **Step 5: Commit.** `feat(core): an expired sign-in is reported as Expired and forgotten`

### Task 4: The app handles an expired sign-in the same way everywhere

**Files:**
- Create: `app/src-tauri/src/session.rs`
- Modify: `app/src-tauri/src/lib.rs` (`library_refresh`, `install_options`, `install_start`), `app/src-tauri/src/updates.rs` (`queue_update`), `app/src-tauri/src/play.rs`, `app/src/lib/auth/auth.ts`, `app/src/lib/auth/auth.test.ts`

**Interfaces:**
- Consumes: `AuthError::Expired`, `InstallError::Expired`, `Downloads::pause_all`, `LibraryCache::clear`, `AppAuth::set`
- Produces:
  - `pub(crate) const EXPIRED: &str = "Your sign-in has expired. Please sign in again."`
  - `pub(crate) async fn expire(app: &tauri::AppHandle)`. It runs `pause_all`, clears `prepared` and the library cache, calls `auth.set(SignedOut)`, emits `login-failed` with `EXPIRED`, then emits `auth-changed` with `SignedOut`, and logs `sign-in expired`.
  - `pub(crate) fn is_expired_install(e: &InstallError) -> bool`
  - The `library_refresh`, Install and Update commands call `expire` before returning their error when the cause is expired. `play_game` calls it when `PlayError.signed_out` is true.

- [ ] **Step 1: Write the failing tests:**
  - In `session.rs`: `is_expired_install` is true only for `InstallError::Expired`.
  - In `auth.test.ts`, `signed_out_event_clears_the_previous_accounts_games`: with the library store holding a game, emit `auth-changed` `{ kind: "SignedOut" }` (`mockIPC(..., { shouldMockEvents: true })` and `emit`). Afterwards `library.games` is `[]` and `authState` is SignedOut.
  - In `auth.test.ts`, `expired_event_shows_the_message_on_login`: emit `login-failed` with `EXPIRED`, then `auth-changed` SignedOut. `loginError` equals the text.
- [ ] **Step 2: Run them and see them fail.** Run `cargo test -p fishy-client session::` and `npm.cmd test -- auth`. Expected: FAIL.
- [ ] **Step 3: Implement.**
  - In the `auth-changed` listener, call `resetLibrary()` when the payload kind is `SignedOut`.
  - In `expire`, only call `pause_all`: never `take_all`, `cancel` or `stop_for_wipe`. A paused download must resume after signing in again.
  - In `library_refresh`, keep the access-token error as `AuthError` so `Expired` can be told apart from other errors before mapping it to `LIBRARY_ERROR`.
- [ ] **Step 4: Run the tests again.** Run `cargo test --workspace`, `npm.cmd test`, clippy and `npm.cmd run check`. Expected: all PASS and clean.
- [ ] **Step 5: Commit.** `feat: an expired sign-in anywhere pauses downloads and returns to Login`

### Task 5: Messages with a next step; "files are in use"

**Files:**
- Modify:
  - core: `crates/fishy-core/src/install/error.rs`, `crates/fishy-core/src/install/engine.rs` (`write_error`, `rebuild_from_downloads`), `crates/fishy-core/src/auth/error.rs`, `crates/fishy-core/src/launch/error.rs`
  - app: `app/src-tauri/src/lib.rs` (`LIBRARY_ERROR`, the browser message), `app/src-tauri/src/updates.rs` (`UPDATE_FETCH_ERROR`), `app/src-tauri/src/downloads.rs` (any `InstallError::Io` built from an `io::Error`)
  - UI: `app/src/lib/library/library.ts` (its `LIBRARY_ERROR`), `app/src/lib/auth/devMock.ts`, and the tests that quote the old wording

**Interfaces:**
- Produces:
  - `InstallError::FilesInUse(String)`, holding the game title, whose `user_message()` is "Some of {title}'s files are in use. Close the game, then press Retry."
  - `pub fn write_error(install_path: &Path, title: &str, e: std::io::Error, needed: u64, free: u64) -> InstallError`: `raw_os_error()` 32 or 33 gives `FilesInUse(title)`, a full disk gives `NoSpace` as now, and anything else gives `Io`
  - `rebuild_from_downloads` takes `title: &str` and builds its errors through `write_error`

- [ ] **Step 1: Write the failing tests:**
  - `install/error.rs`: extend `user_messages_match_spec` with the new texts for `Network`, `NotInstalled`, `Expired` and `FilesInUse("Bendy")`.
  - `engine.rs`:
    - `a_file_in_use_says_close_the_game`: `write_error(p, "Bendy", io::Error::from_raw_os_error(32), 0, 0)` gives `FilesInUse("Bendy")`; the same with 33.
    - `access_denied_still_names_the_folder`: `from_raw_os_error(5)` gives `Io`.
  - `auth/error.rs`: the `Unexpected` and `Storage` display texts match the table.
  - `launch/error.rs`: the `NotInstalled` and `Spawn("x")` texts match the table.
  - App: `updates.rs` checks `UPDATE_FETCH_ERROR`; `lib.rs` gets a small test module for `LIBRARY_ERROR` and the browser message.
  - UI: `library.test.ts` expects the new `LIBRARY_ERROR` text on a failed refresh.
- [ ] **Step 2: Run them and see them fail.** Run `cargo test --workspace` and `npm.cmd test`. Expected: the new assertions FAIL.
- [ ] **Step 3: Implement** the table from §6f.2 word for word. Pass `&job.title` from `run_job` to `write_error` and to `rebuild_from_downloads`.
- [ ] **Step 4: Run the tests again.** Run `cargo test --workspace`, `npm.cmd test`, clippy and check. Expected: all PASS. Update any test or devMock string that quotes an old wording.
- [ ] **Step 5: Commit.** `feat: every error says what to do next; a running game gets "close the game"`

### Task 6: Crash screen and UI error logging

**Files:**
- Create: `app/src/lib/crash/CrashScreen.svelte`, `app/src/lib/crash/crash.ts`, `app/src/lib/crash/CrashBoundary.svelte`, `app/src/lib/crash/crash.test.ts`, `app/src/lib/crash/Boom.test.svelte` (a test-only component that throws while rendering)
- Modify: `app/src/routes/+layout.svelte`, `app/src-tauri/src/data.rs`, `app/src-tauri/src/lib.rs` (register the command)

**Interfaces:**
- Produces:
  - Rust: `#[tauri::command] pub fn log_ui_error(message: String)`, which logs `tracing::warn!("ui error: {}", cut(message, 500))`
  - Rust: `pub(crate) fn cut(s: &str, max_chars: usize) -> &str`, which cuts on a character boundary
  - TS: `reportUiError(e: unknown): void`. It builds `"<name>: <message>"` (or `String(e)`), cuts it to 500 characters, calls `invoke("log_ui_error", { message })`, and swallows any failure.
  - TS: `installGlobalHandlers(target: Window = window): () => void`, which listens to `error` and `unhandledrejection`, calls `reportUiError` for each, and never shows the crash screen
  - `CrashScreen.svelte` props: `{ onreload?: () => void }`, defaulting to `location.reload()`. It renders the FishLoader fish (small), the heading "Something went wrong", the body text, and the Reload and Open logs folder buttons (`openLogsFolder` from `$lib/settings/data`).
  - `CrashBoundary.svelte` props: `{ children: Snippet; onreload?: () => void }`. It wraps `children` in `<svelte:boundary onerror={reportUiError}>`, with a `failed` snippet that renders `CrashScreen`.
  - `+layout.svelte` wraps everything (the ocean and the content) in `CrashBoundary`, and calls `installGlobalHandlers()` in `onMount`.

- [ ] **Step 1: Write the failing tests:**
  - Rust `data.rs`: `cut_keeps_whole_characters`. `cut("aé".repeat(400), 500)` has 500 characters and is valid UTF-8; a short string is unchanged.
  - `crash.test.ts`:
    - `a_screen_that_throws_shows_the_crash_page`: render `CrashBoundary` with `Boom` as its child (through a small wrapper test component, or `createRawSnippet`). The heading "Something went wrong" and both buttons show, and `log_ui_error` was called once with a message starting `Error: boom`.
    - `reload_calls_onreload`
    - `open_logs_folder_calls_the_command`
    - `logging_that_fails_never_throws`: `mockIPC` throws for `log_ui_error`; `reportUiError(new Error("x"))` doesn't throw, and the crash page still renders.
    - `an_unhandled_rejection_is_logged_and_nothing_else_changes`: `installGlobalHandlers(window)`, then dispatch a `PromiseRejectionEvent` (or an `Event("unhandledrejection")` with a `reason`). `log_ui_error` is called and no crash heading appears.
    - `long_messages_are_cut_to_500`
- [ ] **Step 2: Run them and see them fail.** Run `npm.cmd test -- crash` and `cargo test -p fishy-client data::`. Expected: FAIL.
- [ ] **Step 3: Implement.** Style the page like the Login screen (centered, theme colors), with no new colors.
- [ ] **Step 4: Run the tests again.** Run `npm.cmd test`, `npm.cmd run check`, `cargo test --workspace` and clippy. Expected: all PASS and clean.
- [ ] **Step 5: Commit.** `feat(ui): "Something went wrong" crash screen; UI errors go to the log`

### Task 7: Preview, review, §12, hand check

- [ ] **Step 1:** In the browser preview (`http://localhost:1420/?mock=signedin`):
  - Make the devMock's `library_refresh` fail once (add `?mock=signedin&fail=library`). Check the new message and **Try again**.
  - Make one devMock command answer `EXPIRED` and emit the expiry events (add `?mock=signedin&fail=expired` for `library_refresh`). Check the Login screen shows the message.
  - Trigger the crash page with `?crash=1`: a dev-only throw in `+page.svelte`, guarded by `dev`.
  - Check there are no unexpected console errors.
- [ ] **Step 2:** Run the final whole-branch review and fix what it finds (executing-plans).
- [ ] **Step 3:** Update §12:
  - Under "What's done" for Polish 2, add the new test counts.
  - Add "Still to check by hand (milestone 7, sub-project 2)":
    1. With Wi-Fi off, Refresh shows the new message after about 1.5 s, and with Wi-Fi back **Try again** works.
    2. Updating or verifying a game while it's running says "Some of … files are in use. Close the game…" (for example: start Bendy, mark it outdated as in the milestone 6 check, then press Update).
    3. The log shows `retrying …` lines when Wi-Fi drops mid-refresh.
  - Remove "updating a game while it's running…" from "Small things noticed".
- [ ] **Step 4:** Commit and push, then hand check with the user in `npm.cmd run tauri dev`.
