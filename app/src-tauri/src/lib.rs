mod login_window;

use fishy_core::auth::{parse_auth_code, AuthManager, AuthState, EpicClient, KeyringStore};
use tauri::Manager;
use tokio::sync::Mutex;

/// Login state shared by all commands. `current` is None until startup has run once.
pub struct AppAuth {
    pub manager: AuthManager<KeyringStore>,
    current: Mutex<Option<AuthState>>,
}

impl AppAuth {
    pub async fn set(&self, state: AuthState) {
        *self.current.lock().await = Some(state);
    }
}

const NOT_A_CODE: &str = "That doesn't look like a sign-in code. Paste the authorizationCode value.";

/// Current login state. The first call checks the saved session (and refreshes it if needed).
#[tauri::command]
async fn auth_state(auth: tauri::State<'_, AppAuth>) -> Result<AuthState, String> {
    let mut current = auth.current.lock().await;
    if current.is_none() {
        *current = Some(auth.manager.startup().await);
    }
    Ok(current.clone().unwrap_or(AuthState::SignedOut))
}

/// Re-check the saved session (used by "Try again" when offline).
#[tauri::command]
async fn retry_auth(auth: tauri::State<'_, AppAuth>) -> Result<AuthState, String> {
    let state = auth.manager.startup().await;
    auth.set(state.clone()).await;
    Ok(state)
}

#[tauri::command]
fn start_login(app: tauri::AppHandle) -> Result<(), String> {
    login_window::open(&app).map_err(|_| "Couldn't open the sign-in window. Try the paste-a-code option.".to_string())
}

/// Backup flow: the user pastes the code (or Epic's whole JSON) from their browser.
#[tauri::command]
async fn submit_code(input: String, auth: tauri::State<'_, AppAuth>) -> Result<AuthState, String> {
    let code = parse_auth_code(&input).ok_or_else(|| NOT_A_CODE.to_string())?;
    let state = auth.manager.complete_login(&code).await.map_err(|e| e.to_string())?;
    auth.set(state.clone()).await;
    Ok(state)
}

#[tauri::command]
async fn logout(auth: tauri::State<'_, AppAuth>) -> Result<AuthState, String> {
    let state = auth.manager.logout().await;
    auth.set(state.clone()).await;
    Ok(state)
}

#[tauri::command]
fn open_login_in_browser() -> Result<(), String> {
    login_window::open_in_browser().map_err(|_| "Couldn't open your browser.".to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            app.manage(AppAuth {
                manager: AuthManager::new(EpicClient::new(), KeyringStore::windows(), chrono_now),
                current: Mutex::new(None),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            auth_state,
            retry_auth,
            start_login,
            submit_code,
            logout,
            open_login_in_browser
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn chrono_now() -> fishy_core::chrono::DateTime<fishy_core::chrono::Utc> {
    fishy_core::chrono::Utc::now()
}
