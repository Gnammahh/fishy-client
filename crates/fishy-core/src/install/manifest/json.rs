//! Epic's older JSON manifest (ported from Legendary `models/json_manifest.py`).
//!
//! Numbers are "blobs": a string of 3-digit decimal groups, one byte each, little-endian
//! ("001000" is 1). GUIDs are 32 hex characters read as four big-endian u32 words.
//! Chunk data lives in maps keyed by the GUID string: `ChunkHashList` (blob), `ChunkShaList`
//! (hex), `DataGroupList` (blob), `ChunkFilesizeList` (blob).
use super::{ChunkInfo, ChunkPart, FileEntry, Manifest, ManifestMeta};
use crate::install::{Guid, InstallError};
use serde_json::Value;

/// Legendary's default when a JSON manifest has no window sizes.
const DEFAULT_WINDOW: u32 = 1024 * 1024;
const FIRST_UNSUPPORTED_LEVEL: u32 = 22;

fn bad(why: &str) -> InstallError {
    InstallError::BadManifest(why.into())
}

pub(super) fn parse(bytes: &[u8]) -> Result<Manifest, InstallError> {
    let j: Value = serde_json::from_slice(bytes).map_err(|_| bad("json"))?;
    let text = |key: &str| j.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
    let feature_level = j.get("ManifestFileVersion").and_then(Value::as_str).map_or(Some(13), blob_to_num).ok_or_else(|| bad("version"))? as u32;
    if feature_level >= FIRST_UNSUPPORTED_LEVEL {
        tracing::warn!("manifest: feature level {feature_level} not supported (json)");
        return Err(InstallError::Unsupported);
    }
    let meta = ManifestMeta {
        feature_level,
        app_name: text("AppNameString"),
        build_version: text("BuildVersionString"),
        launch_exe: text("LaunchExeString"),
        launch_command: text("LaunchCommand"),
    };

    let map = |key: &str| j.get(key).and_then(Value::as_object).ok_or_else(|| bad(key));
    let hashes = map("ChunkHashList")?;
    let shas = map("ChunkShaList")?;
    let groups = map("DataGroupList")?;
    let sizes = map("ChunkFilesizeList")?;
    let mut chunks = Vec::with_capacity(hashes.len());
    for (key, hash) in hashes {
        let guid = guid_from_hex(key).ok_or_else(|| bad("guid"))?;
        let sha1: [u8; 20] = hex::decode(lookup(shas, key)?).ok().and_then(|v| v.try_into().ok()).ok_or_else(|| bad("chunk sha"))?;
        chunks.push(ChunkInfo {
            guid,
            hash: hash.as_str().and_then(blob_to_num).ok_or_else(|| bad("chunk hash"))?,
            sha1,
            group: blob_to_num(lookup(groups, key)?).ok_or_else(|| bad("group"))? as u8,
            window_size: DEFAULT_WINDOW,
            file_size: blob_to_num(lookup(sizes, key)?).ok_or_else(|| bad("chunk size"))?,
        });
    }

    let list = j.get("FileManifestList").and_then(Value::as_array).ok_or_else(|| bad("FileManifestList"))?;
    let mut files = Vec::with_capacity(list.len());
    for f in list {
        let filename = f.get("Filename").and_then(Value::as_str).ok_or_else(|| bad("Filename"))?.to_string();
        let sha_bytes = f.get("FileHash").and_then(Value::as_str).and_then(blob_to_bytes).ok_or_else(|| bad("FileHash"))?;
        let sha1: [u8; 20] = sha_bytes.try_into().map_err(|_| bad("FileHash"))?;
        let mut parts = Vec::new();
        for p in f.get("FileChunkParts").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default() {
            let num = |k: &str| p.get(k).and_then(Value::as_str).and_then(blob_to_num).ok_or_else(|| bad("chunk part"));
            let guid = p.get("Guid").and_then(Value::as_str).and_then(guid_from_hex).ok_or_else(|| bad("chunk part"))?;
            parts.push(ChunkPart { guid, offset: num("Offset")? as u32, size: num("Size")? as u32 });
        }
        let executable = f.get("bIsExecutable").and_then(Value::as_bool).unwrap_or(false);
        let install_tags = f
            .get("InstallTags")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).filter(|t| !t.is_empty()).map(str::to_string).collect())
            .unwrap_or_default();
        files.push(FileEntry { filename, sha1, executable, parts, install_tags });
    }
    Ok(Manifest::new(meta, chunks, files))
}

fn lookup<'a>(m: &'a serde_json::Map<String, Value>, key: &str) -> Result<&'a str, InstallError> {
    m.get(key).and_then(Value::as_str).ok_or_else(|| bad("chunk field"))
}

/// Each 3-digit decimal group is one byte.
fn blob_to_bytes(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(3) || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.as_bytes().chunks(3).map(|g| std::str::from_utf8(g).ok()?.parse::<u16>().ok().and_then(|v| u8::try_from(v).ok())).collect()
}

/// The bytes of a blob read as a little-endian number (at most 8 bytes).
fn blob_to_num(s: &str) -> Option<u64> {
    let bytes = blob_to_bytes(s)?;
    if bytes.len() > 8 {
        return None;
    }
    Some(bytes.iter().rev().fold(0u64, |n, &b| (n << 8) | b as u64))
}

fn guid_from_hex(s: &str) -> Option<Guid> {
    let bytes = hex::decode(s).ok()?;
    if bytes.len() != 16 {
        return None;
    }
    let word = |i: usize| u32::from_be_bytes(bytes[i * 4..i * 4 + 4].try_into().expect("4 bytes"));
    Some([word(0), word(1), word(2), word(3)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::manifest::parse_manifest;

    #[test]
    fn blob_to_num_reads_le_groups() {
        assert_eq!(blob_to_num("001000"), Some(1));
        assert_eq!(blob_to_num("000001"), Some(256));
        assert_eq!(blob_to_num("255255"), Some(65535));
        assert_eq!(blob_to_num("01"), None);
        assert_eq!(blob_to_num("abc"), None);
    }

    #[test]
    fn guid_from_hex_big_endian() {
        assert_eq!(guid_from_hex("0000000100ABCDEF00000000FFFFFFFF"), Some([1, 0xABCDEF, 0, 0xFFFF_FFFF]));
        assert_eq!(guid_from_hex("xyz"), None);
    }

    fn blob(n: u64, bytes: usize) -> String {
        (0..bytes).map(|i| format!("{:03}", (n >> (8 * i)) & 0xFF)).collect()
    }

    #[test]
    fn parses_minimal_json_manifest() {
        let g = "0000000100000002000000030000000A";
        let sha_blob: String = (1..=20u8).map(|b| format!("{b:03}")).collect();
        let json = serde_json::json!({
            "ManifestFileVersion": blob(13, 4),
            "AppNameString": "Tidepool",
            "BuildVersionString": "1.0",
            "LaunchExeString": "Tide.exe",
            "LaunchCommand": "",
            "ChunkHashList": { g: blob(0x1A2B, 8) },
            "ChunkShaList": { g: "0102030405060708090a0b0c0d0e0f1011121314" },
            "DataGroupList": { g: blob(7, 1) },
            "ChunkFilesizeList": { g: blob(600, 8) },
            "FileManifestList": [{
                "Filename": "Tide.exe",
                "FileHash": sha_blob,
                "bIsExecutable": true,
                "FileChunkParts": [
                    { "Guid": g, "Offset": blob(0, 4), "Size": blob(100, 4) },
                    { "Guid": g, "Offset": blob(100, 4), "Size": blob(28, 4) }
                ]
            }]
        });
        let m = parse_manifest(json.to_string().as_bytes()).unwrap();
        assert_eq!(m.meta.feature_level, 13);
        assert_eq!(m.meta.app_name, "Tidepool");
        assert_eq!(m.meta.launch_exe, "Tide.exe");
        assert_eq!(m.chunk_dir(), "ChunksV3");
        let c = m.chunk(&[1, 2, 3, 10]).unwrap();
        assert_eq!((c.hash, c.group, c.file_size, c.window_size), (0x1A2B, 7, 600, 1024 * 1024));
        assert_eq!(c.sha1[0], 1);
        assert_eq!(c.sha1[19], 0x14);
        let f = &m.files[0];
        assert!(f.executable);
        assert_eq!(f.sha1[0], 1);
        assert_eq!(f.sha1[19], 20);
        assert_eq!(f.size(), 128);
        assert_eq!(f.parts[1].offset, 100);
    }

    #[test]
    fn reads_install_tags_from_json() {
        let sha_blob: String = (1..=20u8).map(|b| format!("{b:03}")).collect();
        let file = |name: &str, tags: Option<serde_json::Value>| {
            let mut f = serde_json::json!({ "Filename": name, "FileHash": sha_blob, "FileChunkParts": [] });
            if let Some(t) = tags {
                f["InstallTags"] = t;
            }
            f
        };
        let json = serde_json::json!({
            "ManifestFileVersion": blob(13, 4),
            "AppNameString": "Fortnite",
            "BuildVersionString": "1.0",
            "ChunkHashList": {}, "ChunkShaList": {}, "DataGroupList": {}, "ChunkFilesizeList": {},
            "FileManifestList": [file("a.pak", Some(serde_json::json!(["GFP_JunoRoot", ""]))), file("b.pak", None)]
        });
        let m = parse_manifest(json.to_string().as_bytes()).unwrap();
        assert_eq!(m.files[0].install_tags, vec!["GFP_JunoRoot".to_string()]);
        assert!(m.files[1].install_tags.is_empty());
    }

    #[test]
    fn missing_keys_are_bad_manifest() {
        assert!(matches!(parse_manifest(b"{\"AppNameString\": \"x\"}"), Err(InstallError::BadManifest(_))));
        assert!(matches!(parse_manifest(b"{not json"), Err(InstallError::BadManifest(_))));
    }
}
