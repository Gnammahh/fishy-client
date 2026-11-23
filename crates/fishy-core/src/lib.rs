//! Epic Games logic for Fishy Client. No UI code lives here.
pub mod auth;
pub mod epic_import;
pub mod install;
pub mod launch;
pub mod library;
pub mod logs;
pub mod retry;
pub mod running;
pub mod settings;
pub mod store;
pub mod wipe;
pub mod achievements;

/// Re-exported so the app uses the same chrono version.
pub use chrono;
