# Fishy Client — Epic Login (Milestone 2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Sign in with a real Epic account through Epic's own login page in an app window, keep the session in Windows Credential Manager, refresh it automatically, and log out cleanly.

**Architecture:** A new Rust crate `crates/fishy-core` holds all Epic logic: the HTTP client, session types, the session store and the auth manager. It's tested against a fake Epic server (`wiremock`). The Tauri app (`app/src-tauri`) is a thin layer: commands, the login window, and an `auth-changed` event. The Svelte UI only calls commands and renders state. Tokens never enter the main window's JavaScript.

**Tech Stack:** Rust: reqwest (rustls, json, form), serde, chrono, thiserror, keyring 3, tokio; dev: wiremock. Tauri 2 (WebviewWindowBuilder, on_navigation, initialization_script). Svelte 5 + Vitest + @testing-library/svelte + `@tauri-apps/api/mocks`.

**Spec:** `docs/superpowers/specs/2026-11-20-fishy-client-design.md` §6a (login) plus §2/§4 for visuals. The visual reference is still `design/mockup.html` (sidebar and buttons).

## Global Constraints
- Epic host, client id/secret, endpoints, form fields and User-Agent exactly as in spec §6a.
- Credential Manager entry: service `FishyClient`, account `epic-session`.
- Refresh threshold: 5 minutes before `expires_at`.
- Login window label `epic-login`, title "Sign in with Epic", 480×720. Capture URL prefix `http://fishy-auth.localhost/done`.
- Capabilities only for window `main`. Remove `tauri-plugin-opener`. CSP exactly as in spec §6a.
- User-facing error copy exactly as in spec §6a.
- Secret values must never appear in `Debug`/`Display` output.
- GPL-3.0-or-later. Windows only.

## Review Focus
1. **Epic returns an error JSON with HTTP 200 or 400** (`errorCode`) → must become `AuthError::Rejected`, never a panic or a bogus session. → test in Task 2.
2. **Clock past both `expires_at` and `refresh_expires_at`** at startup → session cleared, signed out, with no network call. → test in Task 3.
3. **Network down during startup refresh** → session kept, state `Offline`, not signed out. → test in Task 3.
4. **Pasted text with whitespace or quotes, the whole JSON, or a `redirectUrl` JSON with a null code** → the code is extracted, or a clear "not a code" error. → test in Task 1.
5. **Login window closed without finishing, or opened twice** → no stuck loader, and a second click focuses the existing window. → Task 4 handles it, Task 6 checks it by hand.

---

### Task 1: fishy-core crate, session types, code parsing

**Files:** create `crates/fishy-core/{Cargo.toml,src/lib.rs,src/auth/mod.rs,src/auth/session.rs,src/auth/code.rs}`; modify the root `Cargo.toml` (members += `crates/fishy-core`).

**Interfaces (Produces):**
- `pub struct Secret(String)`: `Debug`/`Display` print `[redacted]`; `expose(&self) -> &str`; serde-transparent.
- `pub struct Session { access_token: Secret, refresh_token: Secret, expires_at: DateTime<Utc>, refresh_expires_at: DateTime<Utc>, account_id: String, display_name: String }`. All fields are pub. Deserializes from Epic's token JSON (`displayName` → `display_name`) and round-trips through serde_json for storage.
- `pub fn parse_auth_code(input: &str) -> Option<String>`: trims whitespace and quotes. It accepts a bare code (`[0-9a-f]{32}`) or any JSON object with a non-null string `authorizationCode`, and returns `None` otherwise.
- `pub fn code_from_capture_url(url: &str) -> Option<String>`: returns the `code` query value when the URL starts with `http://fishy-auth.localhost/done`, otherwise `None`.

- [ ] **Tests** (`session.rs`, `code.rs`):
  - `secret_is_redacted_in_debug`: `format!("{:?}", session)` contains neither token value and does contain `[redacted]`
  - `session_parses_epic_token_json`: a fixture with all six fields plus extras such as `token_type` and `client_id` → correct values, and `expires_at` parsed from `"2026-11-20T20:00:00.000Z"`
  - `session_round_trips_json`
  - `parse_bare_code`: `"  a1b2c3d4e5f60718293a4b5c6d7e8f90\n"` → `Some("a1b2…90")`
  - `parse_quoted_code`: the same code wrapped in `"…"` → `Some`
  - `parse_full_redirect_json`: `{"warning":"…","redirectUrl":"https://…","authorizationCode":"a1b2…90","exchangeCode":null,"sid":null}` → `Some`
  - `parse_rejects_null_code_and_junk`: `{"authorizationCode":null}`, `"hello"`, and `""` → `None`
  - `capture_url_extracts_code` / `capture_url_ignores_other_hosts` (`https://www.epicgames.com/…?code=x` → `None`)
- [ ] Run `cargo test -p fishy-core` → FAIL (missing). Implement. → PASS. Commit `feat(core): session types and auth code parsing`.

### Task 2: Epic HTTP client

**Files:** `crates/fishy-core/src/auth/client.rs`, `src/auth/error.rs`.

**Interfaces (Produces):**
- `pub enum AuthError { Network(String), Rejected { code: String, message: String }, Unexpected(String) }` (thiserror). `Display` never includes tokens.
- `pub struct EpicClient`: `EpicClient::new()` uses the real host, and `EpicClient::with_base_url(url: &str)` is for tests.
  - `async fn exchange_code(&self, code: &str) -> Result<Session, AuthError>`
  - `async fn refresh(&self, refresh_token: &Secret) -> Result<Session, AuthError>`
  - `async fn kill_session(&self, access_token: &Secret) -> Result<(), AuthError>`
  - Every request sends the spec User-Agent. Token calls send Basic auth with the client id and secret, form-encoded.
  - Any response body containing `errorCode`, or any non-2xx status → `Rejected` (`code`/`message` taken from the JSON when present). A transport failure → `Network`. A 2xx body that doesn't parse as a `Session` → `Unexpected`.

- [ ] **Tests** (wiremock, `#[tokio::test]`):
  - `exchange_code_posts_form_with_basic_auth`: the mock matches method POST, path `/account/api/oauth/token`, header `authorization: Basic MzRhMDJjZjhmNDQxNGUyOWIxNTkyMTg3NmRhMzZmOWE6ZGFhZmJjY2M3Mzc3NDUwMzlkZmZlNTNkOTRmYzc2Y2Y=`, the user-agent, and body containing `grant_type=authorization_code`, `code=abc` and `token_type=eg1` → returns the fixture, and the result's display_name matches
  - `refresh_sends_refresh_grant`
  - `error_json_is_rejected`: a 400 with `{"errorCode":"errors.com.epicgames.account.oauth.authorization_code_not_found","errorMessage":"Sorry the authorization code you supplied was not found."}` → `Rejected { code: "errors.com…not_found", .. }`
  - `error_json_with_200_is_rejected` *(Review Focus 1)*
  - `connection_refused_is_network`: base URL `http://127.0.0.1:9` → `Network`
  - `kill_session_sends_delete_with_bearer`
- [ ] FAIL → implement → PASS. Commit `feat(core): Epic auth HTTP client`.

### Task 3: Session store and auth manager

**Files:** `crates/fishy-core/src/auth/store.rs`, `src/auth/manager.rs`.

**Interfaces (Produces):**
- `pub trait SessionStore: Send + Sync { fn load(&self) -> Result<Option<Session>, String>; fn save(&self, s: &Session) -> Result<(), String>; fn clear(&self) -> Result<(), String>; }`
- `pub struct MemoryStore` (tests) and `pub struct KeyringStore` (service `FishyClient`, account `epic-session`, value = session JSON; "no entry" → `Ok(None)`).
- `#[derive(Serialize, Clone, Debug, PartialEq)] #[serde(tag = "kind")] pub enum AuthState { SignedOut, SignedIn { display_name: String, account_id: String }, Offline { display_name: String } }`
- `pub struct AuthManager<S: SessionStore>`: `new(client: EpicClient, store: S, now: fn() -> DateTime<Utc>)`.
  - `async fn startup(&self) -> AuthState`: no session → SignedOut. Refresh token expired → clear, SignedOut, **no HTTP call**. Access token expires within 5 minutes → refresh; on success save and return SignedIn; `Rejected` → clear, SignedOut; `Network` → Offline (keep). Otherwise SignedIn.
  - `async fn complete_login(&self, code: &str) -> Result<AuthState, AuthError>`: exchange, save, SignedIn.
  - `async fn logout(&self) -> AuthState`: kill the session (ignore errors), clear, SignedOut.

- [ ] **Tests** (MemoryStore + wiremock + fixed clock):
  - `startup_without_session_is_signed_out`
  - `startup_with_fresh_session_skips_refresh` (the mock expects 0 calls)
  - `startup_refreshes_when_expiring_soon` (expires in 4 min → 1 refresh call, new session saved)
  - `startup_with_dead_refresh_token_clears_without_network` *(Review Focus 2; mock expects 0 calls)*
  - `startup_rejected_refresh_signs_out_and_clears`
  - `startup_network_error_is_offline_and_keeps_session` *(Review Focus 3)*
  - `complete_login_saves_session`
  - `logout_clears_even_if_kill_fails` (the kill mock returns 500)
- [ ] FAIL → implement → PASS. Commit `feat(core): session store and auth manager`.

### Task 4: Tauri wiring, login window, lockdown

**Files:** modify `app/src-tauri/Cargo.toml` (+ `fishy-core = { path = "../../crates/fishy-core" }`, `tokio`; − `tauri-plugin-opener`), `app/src-tauri/src/lib.rs`, a new `app/src-tauri/src/login_window.rs`, `app/src-tauri/capabilities/default.json` (windows `["main"]`, permissions `["core:default"]`), `app/src-tauri/tauri.conf.json` (CSP), and `app/package.json` (− `@tauri-apps/plugin-opener`).

**Interfaces (Produces):** Tauri commands, all returning `Result<AuthState, String>` unless noted:
- `auth_state()`: runs `startup()` once and caches it
- `start_login()` → `()`: opens or focuses `epic-login`
- `submit_code(input: String)`: `parse_auth_code`, then `complete_login`. Errors are the spec's user-facing strings.
- `logout()`
- `open_login_in_browser()` → `()`: runs `cmd /C start "" <login url>`, with the Epic login URL as the only possible argument
- The event `auth-changed`, with an `AuthState` payload, is emitted after a login-window login completes. The event `login-window-closed` is emitted when the window closes without a code.
- `login_window.rs`: `const LOGIN_URL`, plus an initialization script that runs only when `location.host === "www.epicgames.com" && location.pathname === "/id/api/redirect"`. It parses `document.body.innerText` as JSON and, when `authorizationCode` is a string, sets `location.href = "http://fishy-auth.localhost/done?code=" + encodeURIComponent(code)`. `on_navigation` uses `code_from_capture_url`: a match returns `false` (cancel), closes the window, and spawns `complete_login`. Everything else returns `true`.

- [ ] `cargo build -p fishy-client` → OK. `cargo test --workspace` → all core tests pass. `npm run build` → OK. Commit `feat(app): Epic login window, auth commands, CSP lockdown`.

### Task 5: Login screen and app shell (UI)

**Files:** create `app/src/lib/auth/auth.ts` (store + command wrappers), `app/src/lib/auth/LoginScreen.svelte`, `app/src/lib/shell/AppShell.svelte`, `app/src/lib/shell/Sidebar.svelte`, `app/src/lib/ui/PixelButton.svelte` (the mockup's button, variants `primary|ghost|play`), and `app/src/routes/preview/+page.svelte` (moved from `+page.svelte`). Modify `app/src/routes/+page.svelte`.

**Interfaces (Produces):**
- `auth.ts`: `authState: Writable<AuthState | { kind: "Loading" }>`, plus `init()`, `signIn()`, `submitCode(text)`, `signOut()`, `openBrowserLogin()`, which wrap `invoke` and listen to `auth-changed` and `login-window-closed`. `type AuthState` mirrors the Rust enum (`kind` tag).
- `+page.svelte`: Loading → a large FishLoader; SignedOut → LoginScreen; SignedIn/Offline → AppShell. Offline shows a banner with the "Couldn't reach Epic…" text and **Try again**.
- AppShell pages: Library (placeholder copy from spec §6a), Downloads, Settings (placeholders), Account (name + **Log out**).

- [ ] **Tests** (Vitest + `mockIPC`):
  - `login_screen_shows_sign_in_button`
  - `sign_in_button_calls_start_login_and_shows_loader`
  - `paste_fallback_submits_text_and_shows_error_message` (the mock rejects with "That doesn't look like a sign-in code. Paste the authorizationCode value." → the text is visible)
  - `shell_shows_signed_in_name`
  - `account_log_out_calls_logout`
- [ ] FAIL → implement → PASS. `svelte-check` 0/0. Visual check in the browser pane with mocked IPC (login screen and shell match the mockup look). Commit `feat(ui): login screen and app shell`.

### Task 6: Real sign-in (the user does this)
- [ ] The user runs `npm run tauri dev`, clicks **Sign in with Epic** and signs in with their own account. Expected: the window closes and the shell shows their display name.
- [ ] The user restarts the app. Expected: still signed in.
- [ ] The user closes the login window midway. Expected: back on the login screen with no stuck loader. *(Review Focus 5)*
- [ ] The user logs out. Expected: the login screen; Credential Manager has no `FishyClient` entry.
- [ ] Add the README honesty note from spec §6a. Commit `docs: login note`.
