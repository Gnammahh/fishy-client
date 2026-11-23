//! The Store (spec §6o): this week's free games, and Epic's real store in its own window.
use crate::AppLibrary;
use fishy_core::store::{parse_free_games, store_nav_allowed, FreeGames, StoreClient, STORE_HOME};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::webview::NewWindowResponse;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

pub const LABEL: &str = "epic-store";
const LOAD_ERROR: &str = "Couldn't load the free games. Check your internet, then press Try again.";
/// How long the free games list is kept before it's asked for again.
const KEEP: Duration = Duration::from_secs(10 * 60);

pub struct AppStore {
    client: StoreClient,
    /// The last list Epic sent, as it came (parsed per call, so "In your Library" is current).
    saved: Mutex<Option<(Instant, Vec<u8>)>>,
    /// Set right before Fishy Client itself closes the window (Log out): that close isn't
    /// announced, so no Library refresh runs while signing out.
    quiet_close: AtomicBool,
    /// One window build at a time (two quick clicks would build it twice).
    opening: tokio::sync::Mutex<()>,
}

impl AppStore {
    pub fn new() -> Self {
        Self { client: StoreClient::new(), saved: Mutex::new(None), quiet_close: AtomicBool::new(false), opening: tokio::sync::Mutex::new(()) }
    }
}

/// Whether a close of the window should refresh the Library (not when Fishy Client closed it).
fn announce_close(quiet: &AtomicBool) -> bool {
    !quiet.swap(false, Ordering::SeqCst)
}

/// Whether the saved list can be used instead of asking Epic again.
fn fresh(saved: Instant, now: Instant, force: bool) -> bool {
    !force && now.duration_since(saved) < KEEP
}

#[tauri::command]
pub async fn store_free_games(force: bool, store: tauri::State<'_, AppStore>, library: tauri::State<'_, AppLibrary>) -> Result<FreeGames, String> {
    let kept = store.saved.lock().unwrap_or_else(|e| e.into_inner()).clone().filter(|(at, _)| fresh(*at, Instant::now(), force));
    let bytes = match kept {
        Some((_, b)) => b,
        None => {
            let b = store.client.free_games_json().await.map_err(|_| {
                tracing::warn!("store: couldn't load the free games");
                LOAD_ERROR.to_string()
            })?;
            *store.saved.lock().unwrap_or_else(|e| e.into_inner()) = Some((Instant::now(), b.clone()));
            b
        }
    };
    let owned: HashSet<String> = library.catalog_item_ids();
    let games = parse_free_games(&bytes, fishy_core::chrono::Utc::now(), &owned).map_err(|_| {
        tracing::warn!("store: couldn't load the free games");
        LOAD_ERROR.to_string()
    })?;
    tracing::info!("store: free games loaded ({} now, {} next)", games.now.len(), games.next.len());
    Ok(games)
}

/// Opens Epic's store (at `url` when it's an Epic page), or brings the window to the front there.
/// Async: building a window from a synchronous command can hang on Windows.
#[tauri::command]
pub async fn store_open(app: AppHandle, url: Option<String>, store: tauri::State<'_, AppStore>) -> Result<(), String> {
    let _one_at_a_time = store.opening.lock().await;
    let target = url.filter(|u| store_nav_allowed(u)).unwrap_or_else(|| STORE_HOME.to_string());
    let parsed: tauri::Url = target.parse().map_err(|_| OPEN_ERROR.to_string())?;
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.navigate(parsed);
        let _ = w.unminimize();
        let _ = w.set_focus();
        return Ok(());
    }
    let window = WebviewWindowBuilder::new(&app, LABEL, WebviewUrl::External(parsed))
        .title("Epic Games Store")
        .inner_size(1200.0, 800.0)
        .resizable(true)
        // Not private: it shares the main window's browser data, so Epic's sign-in is remembered
        // until Log out clears it (spec §6o). It has no IPC access (capabilities: main only).
        // Payment pop-ups share its session, like in the login window.
        .on_new_window(|_, _| NewWindowResponse::Allow)
        // Only Epic's own https pages show in this window (spec §6o); payment providers come as
        // pop-ups, which on_new_window allows.
        .on_navigation(|u| store_nav_allowed(u.as_str()))
        .build()
        .map_err(|_| OPEN_ERROR.to_string())?;
    tracing::info!("store: window opened");
    let closed = app.clone();
    window.on_window_event(move |event| {
        if matches!(event, WindowEvent::Destroyed) {
            tracing::info!("store: window closed");
            if announce_close(&closed.state::<AppStore>().quiet_close) {
                let _ = closed.emit_to("main", "store-closed", ());
            }
        }
    });
    Ok(())
}

/// Closes the Store window without the Library refresh a user's close brings (Log out, Delete
/// all my data).
pub fn close_quietly(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        app.state::<AppStore>().quiet_close.store(true, Ordering::SeqCst);
        if w.destroy().is_err() {
            app.state::<AppStore>().quiet_close.store(false, Ordering::SeqCst);
        }
    }
}

const OPEN_ERROR: &str = "Couldn't open Epic's store. Try again.";

/// Log out: the Store window closes and the shared browser data (Epic's website sign-in, cached
/// covers) is cleared, so the Store is signed out too (spec §6o).
pub fn sign_out(app: &AppHandle) {
    close_quietly(app);
    // The Store window shares the main window's WebView2 profile, so this clears its cookies too.
    if app.get_webview_window("main").is_some_and(|main| main.clear_all_browsing_data().is_err()) {
        tracing::warn!("store: couldn't clear the browser data");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_close_by_log_out_is_not_announced() {
        let quiet = std::sync::atomic::AtomicBool::new(false);
        assert!(announce_close(&quiet), "the user closed it: refresh the Library");
        quiet.store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(!announce_close(&quiet), "Log out closed it: no refresh");
        assert!(announce_close(&quiet), "only that one close is quiet");
    }

    #[test]
    fn store_cache_is_fresh_for_10_minutes() {
        let saved = Instant::now();
        assert!(fresh(saved, saved + Duration::from_secs(9 * 60), false));
        assert!(!fresh(saved, saved + Duration::from_secs(10 * 60), false));
        assert!(!fresh(saved, saved + Duration::from_secs(1), true), "Try again always fetches");
    }
}
