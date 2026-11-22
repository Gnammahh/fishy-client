//! An expired sign-in, handled the same way wherever it's noticed (spec §6f.2).
use crate::{AppAuth, AppInstall, AppLibrary};
use fishy_core::auth::AuthState;
use fishy_core::install::InstallError;
use tauri::{Emitter, Manager};

pub(crate) const EXPIRED: &str = "Your sign-in has expired. Please sign in again.";

pub(crate) fn is_expired_install(e: &InstallError) -> bool {
    matches!(e, InstallError::Expired)
}

/// The sign-in is over: downloads pause (never cancelled, so Resume works after signing in
/// again), the saved library and prepared Install boxes go, and the UI shows Login with the
/// expired message. The Credential Manager entry was already removed by the auth manager.
pub(crate) async fn expire(app: &tauri::AppHandle) {
    tracing::info!("sign-in expired");
    let install = app.state::<AppInstall>();
    install.downloads.pause_all(app);
    install.prepared.lock().unwrap_or_else(|e| e.into_inner()).clear();
    let _ = app.state::<AppLibrary>().cache.clear();
    app.state::<AppAuth>().set(AuthState::SignedOut).await;
    let _ = app.emit("login-failed", EXPIRED);
    let _ = app.emit("auth-changed", AuthState::SignedOut);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_expired_error_counts() {
        assert!(is_expired_install(&InstallError::Expired));
        assert!(!is_expired_install(&InstallError::Rejected("x".into())));
        assert!(!is_expired_install(&InstallError::Network("x".into())));
        assert_eq!(InstallError::Expired.user_message(), EXPIRED);
    }
}
