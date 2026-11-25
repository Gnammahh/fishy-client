//! The "Sign in with Epic" window: shows Epic's real login page and catches the
//! one-time authorization code without ever handing it to the main window's JavaScript.

use crate::AppAuth;
use fishy_core::auth::{AuthState, CaptureGuard, Navigation};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::webview::NewWindowResponse;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

/// Login events only go to the main window (the login window can't listen anyway).
const MAIN: &str = "main";

pub const LABEL: &str = "epic-login";

/// Epic login page that ends on the JSON redirect containing `authorizationCode` (spec §6a).
pub use fishy_core::epic::LOGIN_URL;

/// Runs on every page in the login window. Only on Epic's redirect page does it read the
/// JSON and hand the code to a local address that Rust intercepts and cancels.
const CAPTURE_SCRIPT: &str = r#"
(function () {
  if (location.host !== "{LOGIN_HOST}" || location.pathname !== "{REDIRECT_PATH}") return;
  function run() {
    try {
      var data = JSON.parse(document.body.innerText);
      if (typeof data.authorizationCode === "string" && data.authorizationCode) {
        location.href = "http://fishy-auth.localhost/done?code=" + encodeURIComponent(data.authorizationCode);
      }
    } catch (e) {}
  }
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", run);
  else run();
})();
"#;

/// [`CAPTURE_SCRIPT`] for Epic's current login host and redirect page.
fn capture_script() -> String {
    CAPTURE_SCRIPT.replace("{LOGIN_HOST}", fishy_core::epic::LOGIN_HOST).replace("{REDIRECT_PATH}", fishy_core::epic::REDIRECT_PATH)
}

/// Opens the login window, or focuses it if it's already open.
pub fn open(app: &AppHandle) -> tauri::Result<()> {
    if let Some(existing) = app.get_webview_window(LABEL) {
        existing.set_focus()?;
        return Ok(());
    }

    let captured = Arc::new(AtomicBool::new(false));
    let nav_app = app.clone();
    let nav_captured = captured.clone();
    let guard = Mutex::new(CaptureGuard::default());

    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::External(LOGIN_URL.parse().expect("login url")))
        .title("Sign in with Epic")
        .inner_size(480.0, 720.0)
        .resizable(true)
        .initialization_script(capture_script())
        // Private (InPrivate) window: Epic's website cookies are never kept, so after
        // Log out nobody on this PC lands in your Epic account by clicking Sign in.
        .incognito(true)
        // "Sign in with Google/Apple/Steam/…" opens a pop-up. Allow it: pop-ups have no IPC
        // access, share this window's private session, and the code is still only accepted
        // here, right after Epic's redirect page (CaptureGuard).
        .on_new_window(|_, _| NewWindowResponse::Allow)
        .on_navigation(move |url| {
            let decision = guard.lock().map(|mut g| g.on_navigate(url.as_str())).unwrap_or(Navigation::Block);
            let code = match decision {
                Navigation::Allow => return true,
                Navigation::Block => return false,
                Navigation::Capture(code) => code,
            };
            if nav_captured.swap(true, Ordering::SeqCst) {
                return false; // already handling a code
            }
            let app = nav_app.clone();
            tauri::async_runtime::spawn(async move {
                if let Some(w) = app.get_webview_window(LABEL) {
                    let _ = w.close();
                }
                finish_login(&app, &code).await;
            });
            false
        })
        .build()?;

    let closed_app = app.clone();
    window.on_window_event(move |event| {
        if matches!(event, WindowEvent::Destroyed) && !captured.load(Ordering::SeqCst) {
            let _ = closed_app.emit_to(MAIN, "login-window-closed", ());
        }
    });
    Ok(())
}

/// Exchanges the code and tells the UI how it went.
pub async fn finish_login(app: &AppHandle, code: &str) {
    let auth = app.state::<AppAuth>();
    match auth.manager.complete_login(code).await {
        Ok(state) => {
            tracing::info!("signed in");
            auth.set(state.clone()).await;
            let _ = app.emit_to(MAIN, "auth-changed", state);
        }
        Err(e) => {
            tracing::warn!("sign-in failed"); // Epic's text can quote secrets, so no details
            let _ = app.emit_to(MAIN, "login-failed", e.to_string());
            let _ = app.emit_to(MAIN, "auth-changed", AuthState::SignedOut);
        }
    }
}

/// Backup flow: open Epic's login in the user's normal browser.
/// Uses the Windows URL handler directly, so no shell ever parses the URL.
pub fn open_in_browser() -> std::io::Result<()> {
    std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", LOGIN_URL]).spawn().map(|_| ())
}
