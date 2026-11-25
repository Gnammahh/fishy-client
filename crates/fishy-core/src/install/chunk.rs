//! Download chunks (ported from Legendary `models/chunk.py`). All integers little-endian.
//!
//! Header: u32 magic 0xB1FE3AA2, u32 header_version, u32 header_size, u32 compressed_size,
//! guid (4×u32), u64 rolling hash, u8 stored_as (1 = zlib, 2 = encrypted); from version 2:
//! [20] SHA-1, u8 hash_type; from version 3: u32 uncompressed_size; from version 4 a 16-byte
//! secret guid and a 16-byte tag (level 22+ games). Data runs from header_size
//! to the end. We check the inflated data against the manifest's SHA-1 for that chunk.
use super::manifest::ChunkInfo;
use super::InstallError;
use sha1::{Digest, Sha1};
use std::io::Read;

/// Where a chunk lives under the CDN base URL (Legendary `ChunkInfo.path`). Level 22+
/// (`ChunksV5`): `plain/` (we only download unencrypted chunks), then the hash and guid as
/// unpadded url-safe base64 of their little-endian bytes. Older: hex.
pub fn chunk_path(dir: &str, c: &ChunkInfo) -> String {
    if dir == "ChunksV5" {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine;
        let guid: Vec<u8> = c.guid.iter().flat_map(|w| w.to_le_bytes()).collect();
        return format!("{dir}/plain/{:02}/{}_{}.chunk", c.group, URL_SAFE_NO_PAD.encode(c.hash.to_le_bytes()), URL_SAFE_NO_PAD.encode(guid));
    }
    let [a, b, d, e] = c.guid;
    format!("{dir}/{:02}/{:016X}_{a:08X}{b:08X}{d:08X}{e:08X}.chunk", c.group, c.hash)
}

const MAGIC: u32 = 0xB1FE_3AA2;
const COMPRESSED: u8 = 0x1;
const ENCRYPTED: u8 = 0x2;

fn bad(why: &str) -> InstallError {
    InstallError::BadChunk(why.into())
}

/// Logs a chunk's format byte the first time chunks with that byte won't inflate, so a new
/// format shows in the log once instead of hiding behind "damaged" retries. Not sensitive.
fn note_inflate_failure(stored_as: u8) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEEN: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];
    let bit = 1u64 << (stored_as % 64);
    if SEEN[usize::from(stored_as / 64)].fetch_or(bit, Ordering::Relaxed) & bit == 0 {
        tracing::warn!("chunk: couldn't inflate (stored_as {stored_as})");
    }
}

/// Checks the header, inflates the data and verifies it against the manifest.
pub fn decode_chunk(bytes: &[u8], expected: &ChunkInfo) -> Result<Vec<u8>, InstallError> {
    let mut r = super::reader::Reader::new(bytes);
    // Any read error here means a damaged or wrong download (e.g. an HTML error page).
    let header = (|| -> Result<(u32, usize, u8), InstallError> {
        let magic = r.u32()?;
        let _version = r.u32()?;
        let header_size = r.u32()? as usize;
        let _compressed_size = r.u32()?;
        let _guid = r.guid()?;
        let _hash = r.u64()?;
        let stored_as = r.u8()?;
        Ok((magic, header_size, stored_as))
    })();
    let (magic, header_size, stored_as) = header.map_err(|_| bad("header"))?;
    if magic != MAGIC {
        return Err(bad("magic"));
    }
    if stored_as & ENCRYPTED != 0 {
        return Err(InstallError::Unsupported);
    }
    let payload = bytes.get(header_size..).ok_or_else(|| bad("truncated"))?;
    let data = if stored_as & COMPRESSED != 0 {
        // Never inflate past the size the manifest states (a "zip bomb" would fill memory).
        let limit = expected.window_size as u64;
        let mut out = Vec::with_capacity(expected.window_size as usize);
        flate2::read::ZlibDecoder::new(payload).take(limit + 1).read_to_end(&mut out).map_err(|_| {
            note_inflate_failure(stored_as);
            bad("zlib")
        })?;
        if out.len() as u64 > limit {
            return Err(bad("too big"));
        }
        out
    } else {
        payload.to_vec()
    };
    if <[u8; 20]>::from(Sha1::digest(&data)) != expected.sha1 {
        return Err(bad("sha1"));
    }
    Ok(data)
}

/// Test helper: a version-3 chunk file holding `data`.
#[cfg(test)]
pub(crate) fn build_chunk(guid: crate::install::Guid, data: &[u8], compress: bool, stored_as: Option<u8>) -> Vec<u8> {
    use std::io::Write;
    let payload = if compress {
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(data).unwrap();
        z.finish().unwrap()
    } else {
        data.to_vec()
    };
    let mut out = Vec::new();
    out.extend(0xB1FE_3AA2u32.to_le_bytes());
    out.extend(3u32.to_le_bytes());
    out.extend(66u32.to_le_bytes());
    out.extend((payload.len() as u32).to_le_bytes());
    for w in guid {
        out.extend(w.to_le_bytes());
    }
    out.extend(0u64.to_le_bytes());
    out.push(stored_as.unwrap_or(if compress { 1 } else { 0 }));
    out.extend(<[u8; 20]>::from(Sha1::digest(data)));
    out.push(2);
    out.extend((data.len() as u32).to_le_bytes());
    out.extend(payload);
    out
}

/// Test helper: the ChunkInfo a manifest would list for `data`.
#[cfg(test)]
pub(crate) fn info_for(guid: crate::install::Guid, data: &[u8], download_size: u64) -> ChunkInfo {
    ChunkInfo { guid, hash: guid[0] as u64, sha1: Sha1::digest(data).into(), group: (guid[0] % 100) as u8, window_size: data.len() as u32, file_size: download_size }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_path_format() {
        let c = ChunkInfo { guid: [0x1, 0xABCDEF, 0, 0xFFFF_FFFF], hash: 0x1A2B, sha1: [0; 20], group: 7, window_size: 0, file_size: 0 };
        assert_eq!(chunk_path("ChunksV4", &c), "ChunksV4/07/0000000000001A2B_0000000100ABCDEF00000000FFFFFFFF.chunk");
    }

    #[test]
    fn chunk_path_for_level_22_and_up() {
        // Checked against Epic's servers with Agent A (2026-11-24): an unencrypted chunk lives
        // under "plain", hash and guid as unpadded url-safe base64 of their little-endian bytes.
        let c = ChunkInfo { guid: [0x1, 0xABCDEF, 0, 0xFFFF_FFFF], hash: 0x1A2B, sha1: [0; 20], group: 7, window_size: 0, file_size: 0 };
        assert_eq!(chunk_path("ChunksV5", &c), "ChunksV5/plain/07/KxoAAAAAAAA_AQAAAO_NqwAAAAAA_____w.chunk");
    }

    #[test]
    fn decodes_compressed_and_plain() {
        let data = b"fish fish fish fish fish".repeat(20);
        let info = info_for([1, 2, 3, 4], &data, 0);
        assert_eq!(decode_chunk(&build_chunk(info.guid, &data, true, None), &info).unwrap(), data);
        assert_eq!(decode_chunk(&build_chunk(info.guid, &data, false, None), &info).unwrap(), data);
    }

    #[test]
    fn wrong_sha_is_bad_chunk() {
        let data = b"fish".to_vec();
        let mut info = info_for([1, 2, 3, 4], &data, 0);
        info.sha1[0] ^= 1;
        assert!(matches!(decode_chunk(&build_chunk(info.guid, &data, true, None), &info), Err(InstallError::BadChunk(_))));
    }

    #[test]
    fn html_page_is_bad_chunk() {
        let info = info_for([1, 2, 3, 4], b"x", 0);
        assert!(matches!(decode_chunk(b"<html>Access denied</html>", &info), Err(InstallError::BadChunk(_))));
        assert!(matches!(decode_chunk(b"", &info), Err(InstallError::BadChunk(_))));
    }

    #[test]
    fn broken_zlib_is_bad_chunk() {
        let data = b"fish".repeat(50);
        let info = info_for([1, 2, 3, 4], &data, 0);
        let mut bytes = build_chunk(info.guid, &data, true, None);
        bytes.truncate(bytes.len() - 5);
        assert!(matches!(decode_chunk(&bytes, &info), Err(InstallError::BadChunk(_))));
    }

    /// A log line goes to `buf` while `f` runs on this thread.
    fn logged(f: impl FnOnce()) -> String {
        use std::sync::{Arc, Mutex};
        #[derive(Clone)]
        struct Buf(Arc<Mutex<Vec<u8>>>);
        impl std::io::Write for Buf {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(b);
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let buf = Buf(Arc::default());
        let w = buf.clone();
        let sub = tracing_subscriber::fmt().with_ansi(false).with_writer(move || w.clone()).finish();
        tracing::subscriber::with_default(sub, f);
        
        String::from_utf8(buf.0.lock().unwrap().clone()).unwrap()
    }

    #[test]
    fn a_chunk_that_wont_inflate_logs_its_format_once() {
        // 0x41: compressed plus a flag no format we know uses (a new codec, say).
        let data = b"kelp".repeat(50);
        let info = info_for([9, 9, 9, 9], &data, 0);
        let mut bytes = build_chunk(info.guid, &data, true, Some(0x41));
        bytes.truncate(bytes.len() - 5);
        let text = logged(|| {
            for _ in 0..3 {
                assert!(matches!(decode_chunk(&bytes, &info), Err(InstallError::BadChunk(_))));
            }
        });
        assert_eq!(text.matches("stored_as 65").count(), 1, "{text}");
    }

    #[test]
    fn inflating_stops_past_the_stated_size() {
        // A small download that would inflate to 64 MB is refused without filling memory.
        let small = b"fish".to_vec();
        let info = info_for([1, 2, 3, 4], &small, 0);
        let bomb = build_chunk(info.guid, &vec![0u8; 64 << 20], true, None);
        assert_eq!(decode_chunk(&bomb, &info), Err(InstallError::BadChunk("too big".into())));
    }

    #[test]
    fn a_version_4_header_is_skipped() {
        // Level 22+ games: the chunk header adds a 16-byte secret guid and a 16-byte tag.
        let data = b"fish data".repeat(10);
        let mut bytes = build_chunk([7, 1, 2, 3], &data, true, None);
        bytes[4..8].copy_from_slice(&4u32.to_le_bytes());
        bytes[8..12].copy_from_slice(&98u32.to_le_bytes());
        bytes.splice(66..66, [0u8; 32]);
        assert_eq!(decode_chunk(&bytes, &info_for([7, 1, 2, 3], &data, bytes.len() as u64)).unwrap(), data);
    }

    #[test]
    fn encrypted_chunk_unsupported() {
        let data = b"fish".to_vec();
        let info = info_for([1, 2, 3, 4], &data, 0);
        assert_eq!(decode_chunk(&build_chunk(info.guid, &data, false, Some(2)), &info), Err(InstallError::Unsupported));
    }
}
