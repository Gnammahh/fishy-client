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

/// Where a chunk lives under the CDN base URL.
pub fn chunk_path(dir: &str, c: &ChunkInfo) -> String {
    let [a, b, d, e] = c.guid;
    format!("{dir}/{:02}/{:016X}_{a:08X}{b:08X}{d:08X}{e:08X}.chunk", c.group, c.hash)
}

const MAGIC: u32 = 0xB1FE_3AA2;
const COMPRESSED: u8 = 0x1;
const ENCRYPTED: u8 = 0x2;

fn bad(why: &str) -> InstallError {
    InstallError::BadChunk(why.into())
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
        flate2::read::ZlibDecoder::new(payload).take(limit + 1).read_to_end(&mut out).map_err(|_| bad("zlib"))?;
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
