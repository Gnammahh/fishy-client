//! Starting an installed game signed in as the user (spec §6d).
mod client;
mod command;
mod error;

pub use command::{build_command, language_from, split_args, LaunchCommand, LaunchInputs};
pub use client::LaunchClient;
pub use error::LaunchError;
