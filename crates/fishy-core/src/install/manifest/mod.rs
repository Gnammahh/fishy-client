//! Epic's game manifest: which files a build has and which chunks they're made of.
mod binary;
mod json;
/// Builds manifests in tests (also for the app's tests, through the `test-support` feature).
#[cfg(any(test, feature = "test-support"))]
pub mod testbuild;

use super::{Guid, InstallError};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ManifestMeta {
    pub feature_level: u32,
    pub app_name: String,
    pub build_version: String,
    pub launch_exe: String,
    pub launch_command: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkInfo {
    pub guid: Guid,
    /// Epic's rolling hash; part of the download path.
    pub hash: u64,
    /// SHA-1 of the uncompressed chunk data.
    pub sha1: [u8; 20],
    pub group: u8,
    /// Uncompressed size.
    pub window_size: u32,
    /// Download (compressed) size.
    pub file_size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkPart {
    pub guid: Guid,
    /// Where in the chunk this part starts.
    pub offset: u32,
    pub size: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    /// Epic's path with `/` separators, relative to the install folder.
    pub filename: String,
    pub sha1: [u8; 20],
    pub executable: bool,
    pub parts: Vec<ChunkPart>,
}

impl FileEntry {
    pub fn size(&self) -> u64 {
        self.parts.iter().map(|p| p.size as u64).sum()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub meta: ManifestMeta,
    pub chunks: Vec<ChunkInfo>,
    pub files: Vec<FileEntry>,
    index: HashMap<Guid, usize>,
}

impl Manifest {
    pub fn new(meta: ManifestMeta, chunks: Vec<ChunkInfo>, files: Vec<FileEntry>) -> Self {
        let index = chunks.iter().enumerate().map(|(i, c)| (c.guid, i)).collect();
        Self { meta, chunks, files, index }
    }

    pub fn chunk(&self, guid: &Guid) -> Option<&ChunkInfo> {
        self.index.get(guid).map(|&i| &self.chunks[i])
    }

    /// Size of the installed game: every file's size added up.
    pub fn install_size(&self) -> u64 {
        self.files.iter().map(FileEntry::size).sum()
    }

    /// CDN folder for chunks, by feature level (Legendary `get_chunk_dir`).
    pub fn chunk_dir(&self) -> &'static str {
        match self.meta.feature_level {
            15.. => "ChunksV4",
            6.. => "ChunksV3",
            3.. => "ChunksV2",
            _ => "Chunks",
        }
    }
}

/// Reads either format: JSON if it starts with `{`, otherwise binary.
pub fn parse_manifest(bytes: &[u8]) -> Result<Manifest, InstallError> {
    if bytes.first() == Some(&b'{') {
        json::parse(bytes)
    } else {
        binary::parse(bytes)
    }
}
