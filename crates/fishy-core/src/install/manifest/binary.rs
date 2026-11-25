//! Epic's binary manifest (ported from Legendary `models/manifest.py`). All integers little-endian.
//!
//! Header: u32 magic 0x44BEC00C, u32 header_size, u32 size_uncompressed, u32 size_compressed,
//! [20] SHA-1 of the uncompressed body, u8 stored_as (1 = zlib, 2 = encrypted), u32 version;
//! version >= 22 adds a 16-byte secret guid and a 16-byte tag. The body starts at header_size.
//!
//! The body is four sections. Each starts with u32 size (counting itself) and u8 version; after
//! reading what we know we always jump to start + size, so newer fields are skipped.
//! - Meta: u32 feature_level, u8 is_file_data, u32 app_id, FString app_name, build_version,
//!   launch_exe, launch_command, u32 n + n FString prereq ids, FString prereq name/path/args, ...
//! - Chunk list: u32 count, then columns: guid (4×u32), u64 hash, [20] sha1, u8 group,
//!   u32 window_size, i64 file_size; level 22+ adds secret guid, compressed window size and
//!   encryption tag columns after those (skipped).
//! - File list: u32 count, then columns: FString filename, FString symlink target, [20] sha1,
//!   u8 flags (0x4 = executable), install tags (u32 n + n FString), chunk parts
//!   (u32 n, each: u32 struct_size, guid, u32 offset, u32 size), newer extras ignored.
//! - Custom fields: ignored.
use super::{ChunkInfo, ChunkPart, FileEntry, Manifest, ManifestMeta};
use crate::install::reader::Reader;
use crate::install::InstallError;
use sha1::{Digest, Sha1};
use std::io::Read;

const MAGIC: u32 = 0x44BE_C00C;
const COMPRESSED: u8 = 0x1;
const ENCRYPTED: u8 = 0x2;
/// Levels 22 to 24 add a longer header and extra chunk columns, which the section sizes let us
/// skip; their chunks live in ChunksV5. Encrypted lists or chunks (the `stored_as` flag) are
/// still refused: they need secrets we don't have. 24 is the highest level Legendary knows.
const FIRST_UNSUPPORTED_LEVEL: u32 = 25;

fn bad(why: &str) -> InstallError {
    InstallError::BadManifest(why.into())
}

pub(super) fn parse(bytes: &[u8]) -> Result<Manifest, InstallError> {
    let mut r = Reader::new(bytes);
    if r.u32()? != MAGIC {
        return Err(bad("magic"));
    }
    let header_size = r.u32()? as usize;
    let _size_uncompressed = r.u32()?;
    let _size_compressed = r.u32()?;
    let sha: [u8; 20] = r.bytes(20)?.try_into().expect("20 bytes");
    let stored_as = r.u8()?;
    let _version = r.u32()?;
    if stored_as & ENCRYPTED != 0 {
        tracing::warn!("manifest: encrypted (binary)");
        return Err(InstallError::Unsupported);
    }
    let raw = bytes.get(header_size..).ok_or_else(|| bad("truncated"))?;
    let body = if stored_as & COMPRESSED != 0 {
        let mut out = Vec::new();
        flate2::read::ZlibDecoder::new(raw).read_to_end(&mut out).map_err(|_| bad("zlib"))?;
        if <[u8; 20]>::from(Sha1::digest(&out)) != sha {
            return Err(bad("hash"));
        }
        out
    } else {
        raw.to_vec()
    };

    let mut r = Reader::new(&body);
    let meta = read_meta(&mut r)?;
    if meta.feature_level >= FIRST_UNSUPPORTED_LEVEL {
        tracing::warn!("manifest: feature level {} not supported (binary)", meta.feature_level);
        return Err(InstallError::Unsupported);
    }
    let chunks = read_chunks(&mut r, meta.feature_level)?;
    let files = read_files(&mut r)?;
    Ok(Manifest::new(meta, chunks, files))
}

/// Reads a section's size and version, runs `read` (told where the section ends), then jumps
/// to the section's end.
fn section<T>(r: &mut Reader, read: impl FnOnce(&mut Reader, usize) -> Result<T, InstallError>) -> Result<T, InstallError> {
    let start = r.pos();
    let size = r.u32()? as usize;
    let _version = r.u8()?;
    let end = start.checked_add(size).ok_or_else(|| bad("section size"))?;
    let value = read(r, end)?;
    if r.pos() > end {
        return Err(bad("section overrun"));
    }
    r.seek(end);
    Ok(value)
}

fn read_meta(r: &mut Reader) -> Result<ManifestMeta, InstallError> {
    section(r, |r, _| {
        let feature_level = r.u32()?;
        let _is_file_data = r.u8()?;
        let _app_id = r.u32()?;
        let app_name = r.fstring()?;
        let build_version = r.fstring()?;
        let launch_exe = r.fstring()?;
        let launch_command = r.fstring()?;
        // Prerequisites (ids, name, path, args) and later fields aren't needed for installing.
        Ok(ManifestMeta { feature_level, app_name, build_version, launch_exe, launch_command })
    })
}

/// Guards against a damaged count making us allocate gigabytes.
fn count(r: &mut Reader, min_bytes_each: usize) -> Result<usize, InstallError> {
    let n = r.u32()? as usize;
    if n.saturating_mul(min_bytes_each) > r.remaining() {
        return Err(bad("count"));
    }
    Ok(n)
}

fn read_chunks(r: &mut Reader, feature_level: u32) -> Result<Vec<ChunkInfo>, InstallError> {
    section(r, |r, end| {
        let n = count(r, 16 + 8 + 20 + 1 + 4 + 8)?;
        let guids = (0..n).map(|_| r.guid()).collect::<Result<Vec<_>, _>>()?;
        let hashes = (0..n).map(|_| r.u64()).collect::<Result<Vec<_>, _>>()?;
        let shas = (0..n).map(|_| Ok(r.bytes(20)?.try_into().expect("20 bytes"))).collect::<Result<Vec<[u8; 20]>, InstallError>>()?;
        let groups = (0..n).map(|_| r.u8()).collect::<Result<Vec<_>, _>>()?;
        let windows = (0..n).map(|_| r.u32()).collect::<Result<Vec<_>, _>>()?;
        let sizes = (0..n).map(|_| r.i64()).collect::<Result<Vec<_>, _>>()?;
        // Level 22+: a chunk with a secret guid is encrypted; we can't decrypt it, so say so now
        // rather than failing mid-download. The other new columns are skipped.
        if feature_level >= 22 {
            // Only from this section: past its end is the file list, not secret guids.
            if r.pos().saturating_add(16 * n) > end {
                return Err(bad("chunk list"));
            }
            let secrets = r.bytes(16 * n)?;
            if secrets.iter().any(|&b| b != 0) {
                tracing::warn!("manifest: encrypted chunks");
                return Err(InstallError::Unsupported);
            }
        }
        Ok((0..n)
            .map(|i| ChunkInfo {
                guid: guids[i],
                hash: hashes[i],
                sha1: shas[i],
                group: groups[i],
                window_size: windows[i],
                file_size: sizes[i].max(0) as u64,
            })
            .collect())
    })
}

fn read_files(r: &mut Reader) -> Result<Vec<FileEntry>, InstallError> {
    section(r, |r, _| {
        let n = count(r, 4 + 4 + 20 + 1 + 4 + 4)?;
        let names = (0..n).map(|_| r.fstring()).collect::<Result<Vec<_>, _>>()?;
        for _ in 0..n {
            r.fstring()?; // symlink target, unused on Windows
        }
        let shas = (0..n).map(|_| Ok(r.bytes(20)?.try_into().expect("20 bytes"))).collect::<Result<Vec<[u8; 20]>, InstallError>>()?;
        let flags = (0..n).map(|_| r.u8()).collect::<Result<Vec<_>, _>>()?;
        let mut tags = Vec::with_capacity(n);
        for _ in 0..n {
            let mut list = Vec::new();
            for _ in 0..count(r, 4)? {
                let tag = r.fstring()?;
                if !tag.is_empty() {
                    list.push(tag); // "" means no part (Fortnite has some)
                }
            }
            tags.push(list);
        }
        let mut parts = Vec::with_capacity(n);
        for _ in 0..n {
            let k = count(r, 28)?;
            let mut list = Vec::with_capacity(k);
            for _ in 0..k {
                let start = r.pos();
                let struct_size = r.u32()? as usize;
                let guid = r.guid()?;
                let offset = r.u32()?;
                let size = r.u32()?;
                if struct_size < 28 {
                    return Err(bad("chunk part size"));
                }
                r.seek(start + struct_size);
                list.push(ChunkPart { guid, offset, size });
            }
            parts.push(list);
        }
        Ok(names
            .into_iter()
            .zip(shas)
            .zip(flags)
            .zip(parts)
            .zip(tags)
            .map(|((((filename, sha1), flag), parts), install_tags)| FileEntry { filename, sha1, executable: flag & 0x4 != 0, parts, install_tags })
            .collect())
    })
}

#[cfg(test)]
mod tests {
    use super::super::testbuild::{build_binary, build_with, sample, BuildOpts};
    use super::super::{parse_manifest, ChunkPart, FileEntry, Manifest};
    use crate::install::InstallError;

    #[test]
    fn round_trips_compressed_and_plain() {
        let m = sample();
        assert_eq!(parse_manifest(&build_binary(&m, true)).unwrap(), m);
        assert_eq!(parse_manifest(&build_binary(&m, false)).unwrap(), m);
    }

    #[test]
    fn sizes_and_lookups() {
        let m = parse_manifest(&build_binary(&sample(), true)).unwrap();
        assert_eq!(m.files[0].size(), 1200);
        assert_eq!(m.files[2].size(), 0);
        assert_eq!(m.install_size(), 1224);
        assert_eq!(m.chunk(&[5, 6, 7, 8]).unwrap().group, 42);
        assert!(m.chunk(&[9, 9, 9, 9]).is_none());
    }

    #[test]
    fn bad_magic_rejected() {
        let bytes = build_with(&sample(), BuildOpts { magic: Some(0x1234), ..Default::default() });
        assert!(matches!(parse_manifest(&bytes), Err(InstallError::BadManifest(_))));
    }

    #[test]
    fn damaged_body_rejected() {
        let mut bytes = build_binary(&sample(), true);
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
        assert!(matches!(parse_manifest(&bytes), Err(InstallError::BadManifest(_))));
        // marked compressed but actually plain: inflating fails
        let bytes = build_with(&sample(), BuildOpts { stored_as: Some(1), ..Default::default() });
        assert!(matches!(parse_manifest(&bytes), Err(InstallError::BadManifest(_))));
    }

    #[test]
    fn encrypted_is_unsupported() {
        let bytes = build_with(&sample(), BuildOpts { stored_as: Some(3), compress: true, ..Default::default() });
        assert_eq!(parse_manifest(&bytes), Err(InstallError::Unsupported));
    }

    #[test]
    fn feature_levels_22_to_24_are_read_when_not_encrypted() {
        // Agent A's file list is level 24 (found 2026-11-24): longer header, extra chunk columns.
        for level in [22, 23, 24] {
            let mut m = sample();
            m.meta.feature_level = level;
            assert_eq!(parse_manifest(&build_binary(&m, true)).unwrap(), m, "level {level}");
        }
    }

    #[test]
    fn a_level_24_list_with_an_encrypted_chunk_is_unsupported() {
        let mut m = sample();
        m.meta.feature_level = 24;
        let bytes = build_with(&m, BuildOpts { compress: true, secret_chunk: true, ..Default::default() });
        assert_eq!(parse_manifest(&bytes), Err(InstallError::Unsupported));
    }

    #[test]
    fn the_secret_guid_column_must_fit_in_the_chunk_list() {
        // Says level 22 but its chunk list has no level 22 columns: the secret guids would be
        // read from the file list after it. That's a damaged list, not an encrypted one.
        let mut m = sample();
        m.meta.feature_level = 21;
        let mut bytes = build_with(&m, BuildOpts { compress: false, ..Default::default() });
        let at = 41 + 4 + 1; // header, then the meta section's size and version
        assert_eq!(bytes[at..at + 4], 21u32.to_le_bytes());
        bytes[at..at + 4].copy_from_slice(&22u32.to_le_bytes());
        assert!(matches!(parse_manifest(&bytes), Err(InstallError::BadManifest(_))), "{:?}", parse_manifest(&bytes));
    }

    #[test]
    fn feature_level_25_is_unsupported() {
        let mut m = sample();
        m.meta.feature_level = 25;
        assert_eq!(parse_manifest(&build_binary(&m, true)), Err(InstallError::Unsupported));
    }

    #[test]
    fn an_encrypted_level_24_list_is_still_unsupported() {
        let mut m = sample();
        m.meta.feature_level = 24;
        let bytes = build_with(&m, BuildOpts { stored_as: Some(3), compress: true, ..Default::default() });
        assert_eq!(parse_manifest(&bytes), Err(InstallError::Unsupported));
    }

    #[test]
    fn unknown_extra_section_bytes_are_skipped() {
        let bytes = build_with(&sample(), BuildOpts { compress: true, meta_padding: 8, ..Default::default() });
        assert_eq!(parse_manifest(&bytes).unwrap(), sample());
    }

    #[test]
    fn utf16_filename() {
        let s = sample();
        let mut files = s.files.clone();
        files.push(FileEntry { install_tags: vec![], filename: "Données/çà.txt".into(), sha1: [9; 20], executable: false, parts: vec![ChunkPart { guid: [1, 2, 3, 4], offset: 0, size: 1 }] });
        let m = Manifest::new(s.meta.clone(), s.chunks.clone(), files);
        assert_eq!(parse_manifest(&build_binary(&m, false)).unwrap(), m);
    }

    #[test]
    fn reads_install_tags() {
        let mut m = sample();
        m.files[0].install_tags = vec!["Langde".into(), "".into()];
        let got = parse_manifest(&build_binary(&m, true)).unwrap();
        assert_eq!(got.files[0].install_tags, vec!["Langde".to_string()], "empty names are dropped");
        assert!(got.files[1].install_tags.is_empty());
    }

    #[test]
    fn chunk_dir_by_feature_level() {
        let mut m = sample();
        for (level, dir) in [(2, "Chunks"), (3, "ChunksV2"), (6, "ChunksV3"), (14, "ChunksV3"), (15, "ChunksV4"), (21, "ChunksV4"), (22, "ChunksV5"), (24, "ChunksV5")] {
            m.meta.feature_level = level;
            assert_eq!(m.chunk_dir(), dir, "level {level}");
        }
    }

    #[test]
    fn empty_input_rejected() {
        assert!(matches!(parse_manifest(&[]), Err(InstallError::BadManifest(_))));
    }
}
