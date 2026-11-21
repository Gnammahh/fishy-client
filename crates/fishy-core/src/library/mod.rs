//! The user's game library: fetching it from Epic, filtering it, caching it.
mod build;
mod cache;
mod client;

pub use build::{build_library, pick_cover, worth_fetching, CatalogItem, Category, Game, KeyImage, LaunchInfo, LibraryItem};
pub use cache::{CachedLibrary, LibraryCache};
pub use client::{LibraryClient, LibraryData};
