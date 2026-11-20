//! Epic Games logic for Fishy Client. No UI code lives here.
pub mod auth;
pub mod install;
pub mod library;

/// Re-exported so the app uses the same chrono version.
pub use chrono;
