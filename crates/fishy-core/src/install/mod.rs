//! Installing games: manifests, chunk downloads, resume, installed list, uninstall (spec §6c).
mod error;
pub(crate) mod reader;
pub mod manifest;
pub mod chunk;
pub mod client;
pub mod plan;

pub use error::{format_bytes, InstallError};

/// A chunk id: four little-endian u32 words, as Epic stores it.
pub type Guid = [u32; 4];
