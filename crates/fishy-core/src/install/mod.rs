//! Installing games: manifests, chunk downloads, resume, installed list, uninstall (spec §6c).
mod error;
pub(crate) mod reader;
pub mod limit;
pub mod manifest;
pub mod chunk;
pub mod client;
pub mod compare;
pub mod elevate;
pub mod engine;
pub mod parts;
pub mod plan;
pub mod space;
pub mod speed;
pub mod store;
pub mod uninstall;

pub use error::{format_bytes, InstallError};

/// A chunk id: four little-endian u32 words, as Epic stores it.
pub type Guid = [u32; 4];
