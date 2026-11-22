//! The Play button's command (spec §6d). The exchange code never leaves fishy-core's launch module
//! except as the game's argument; nothing here logs it.
use crate::running::AppRunning;
use crate::updates::AppSettings;
use crate::{AppAuth, AppInstall, AppLibrary};
use fishy_core::auth::AuthError;
use fishy_core::launch::{clean_ovt_dir, default_ovt_dir, language_from, prepare_launch, start, LaunchClient, LaunchError, Player};
use fishy_core::running::already_running;
use serde::Serialize;
use tauri::Emitter;
use std::path::PathBuf;

pub struct AppLaunch {
    client: LaunchClient,
    ovt_dir: PathBuf,
}

impl AppLaunch {
    /// Also deletes ownership token files left from earlier launches.
    pub fn new() -> Self {
        let ovt_dir = default_ovt_dir();
        clean_ovt_dir(&ovt_dir);
        Self { client: LaunchClient::new(), ovt_dir }
    }
}

/// What the UI gets when Play fails.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct PlayError {
    message: String,
    /// Show "Play offline" (Epic unreachable and the game may run offline).
    can_offline: bool,
    /// The sign-in is gone: the UI goes back to Login.
    signed_out: bool,
}

fn play_error(e: LaunchError, title: &str, game_can_run_offline: bool) -> PlayError {
    PlayError {
        message: e.user_message(title),
        can_offline: e == LaunchError::Network && game_can_run_offline,
        signed_out: e == LaunchError::SignedOut,
    }
}

/// Refreshing the session failed: no internet, or the sign-in is no longer valid.
fn session_error(e: AuthError) -> LaunchError {
    match e {
        AuthError::Network(_) | AuthError::Unexpected(_) => LaunchError::Network,
        AuthError::Rejected { .. } | AuthError::Storage(_) | AuthError::Expired => LaunchError::SignedOut,
    }
}

/// Offline play can't sign in again anyway, so a sign-in problem there never forgets anything.
fn should_expire(signed_out: bool, offline: bool) -> bool {
    signed_out && !offline
}

#[tauri::command]
#[allow(clippy::too_many_arguments)] // Tauri states, one each
pub async fn play_game(
    app: tauri::AppHandle,
    app_name: String,
    offline: bool,
    auth: tauri::State<'_, AppAuth>,
    library: tauri::State<'_, AppLibrary>,
    install: tauri::State<'_, AppInstall>,
    launch: tauri::State<'_, AppLaunch>,
    running: tauri::State<'_, AppRunning>,
    settings: tauri::State<'_, AppSettings>,
) -> Result<(), PlayError> {
    let result = launch_game(&app, app_name.clone(), offline, auth, library, install, launch, running, settings).await;
    if let Err(e) = &result {
        tracing::warn!("play {app_name} failed: {}", e.message);
        if should_expire(e.signed_out, offline) {
            crate::session::expire(&app).await;
        }
    }
    result
}

#[allow(clippy::too_many_arguments)] // Tauri states, one each
async fn launch_game(
    app: &tauri::AppHandle,
    app_name: String,
    offline: bool,
    auth: tauri::State<'_, AppAuth>,
    library: tauri::State<'_, AppLibrary>,
    install: tauri::State<'_, AppInstall>,
    launch: tauri::State<'_, AppLaunch>,
    running: tauri::State<'_, AppRunning>,
    settings: tauri::State<'_, AppSettings>,
) -> Result<(), PlayError> {
    let Some(installed) = install.downloads.store.installed().into_iter().find(|g| g.app_name == app_name) else {
        return Err(play_error(LaunchError::NotInstalled, &app_name, false));
    };
    let title = installed.title.clone();
    if running.contains(&app_name) {
        return Err(PlayError { message: already_running(&title), can_offline: false, signed_out: false });
    }
    let Some(game) = library.cache.load().and_then(|c| c.games.into_iter().find(|g| g.app_name == app_name)) else {
        return Err(play_error(LaunchError::NotInLibrary, &title, false));
    };
    let fail = |e: LaunchError| play_error(e, &title, game.launch.can_run_offline);

    let (token, display_name, account_id) = if offline {
        let (name, id) = auth.manager.saved_identity().ok_or_else(|| fail(LaunchError::SignedOut))?;
        (None, name, id)
    } else {
        let s = auth.manager.session().await.map_err(|e| fail(session_error(e)))?;
        (Some(s.access_token), s.display_name, s.account_id)
    };
    let locale = language_from(sys_locale::get_locale().as_deref());
    let player = Player { display_name: &display_name, account_id: &account_id, locale: &locale };
    let cmd = prepare_launch(&launch.client, token.as_ref(), &player, &installed, &game, &launch.ovt_dir).await.map_err(fail)?;
    start(&cmd).map_err(fail)?;
    tracing::info!("started {app_name}{}", if offline { " (offline)" } else { "" });
    if install.downloads.store.mark_played(&app_name, fishy_core::chrono::Utc::now()).is_err() {
        tracing::warn!("couldn't save play time for {app_name}");
    }
    let _ = app.emit("installed-changed", ());
    crate::running::started(app, &app_name, &installed.install_path, settings.store.load().when_game_starts == "minimize");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_online_sign_in_problem_expires_the_sign_in() {
        assert!(should_expire(true, false));
        assert!(!should_expire(true, true), "offline: can't sign in again anyway, keep everything");
        assert!(!should_expire(false, false));
    }

    #[test]
    fn play_offline_is_offered_only_for_network_errors_on_offline_games() {
        assert!(play_error(LaunchError::Network, "Reef", true).can_offline);
        assert!(!play_error(LaunchError::Network, "Reef", false).can_offline);
        assert!(!play_error(LaunchError::MissingProgram, "Reef", true).can_offline);
    }

    #[test]
    fn expired_sign_in_sends_the_user_to_login() {
        let e = play_error(session_error(AuthError::Rejected { code: "invalid_grant".into() }), "Reef", true);
        assert!(e.signed_out && !e.can_offline);
        let e = play_error(session_error(AuthError::Network("dns".into())), "Reef", true);
        assert!(!e.signed_out && e.can_offline);
        assert_eq!(e.message, "Couldn't reach Epic to sign the game in. Check your internet.");
    }
}
