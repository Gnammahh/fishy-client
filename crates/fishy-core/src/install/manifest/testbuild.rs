//! Test-only writer for the binary manifest, mirroring `binary.rs`.
use super::Manifest;
use sha1::{Digest, Sha1};
use std::io::Write;

#[derive(Default, Clone, Copy)]
pub struct BuildOpts {
    pub compress: bool,
    /// Extra unknown bytes at the end of the meta section (a newer format).
    pub meta_padding: usize,
    pub stored_as: Option<u8>,
    pub magic: Option<u32>,
    /// Level 22+: give the first chunk a secret guid (an encrypted chunk).
    pub secret_chunk: bool,
}

pub fn fstring(out: &mut Vec<u8>, s: &str) {
    if s.is_empty() {
        out.extend(0i32.to_le_bytes());
    } else if s.is_ascii() {
        out.extend((s.len() as i32 + 1).to_le_bytes());
        out.extend(s.as_bytes());
        out.push(0);
    } else {
        let units: Vec<u16> = s.encode_utf16().collect();
        out.extend((-(units.len() as i32 + 1)).to_le_bytes());
        for u in units {
            out.extend(u.to_le_bytes());
        }
        out.extend([0, 0]);
    }
}

fn guid(out: &mut Vec<u8>, g: &[u32; 4]) {
    for w in g {
        out.extend(w.to_le_bytes());
    }
}

/// Writes a section: u32 size (incl. itself), u8 version, then `body`.
fn section(out: &mut Vec<u8>, version: u8, body: &[u8], padding: usize) {
    let size = 4 + 1 + body.len() + padding;
    out.extend((size as u32).to_le_bytes());
    out.push(version);
    out.extend(body);
    out.extend(std::iter::repeat_n(0xEE, padding));
}

pub fn build_binary(m: &Manifest, compress: bool) -> Vec<u8> {
    build_with(m, BuildOpts { compress, ..Default::default() })
}

pub fn build_with(m: &Manifest, opts: BuildOpts) -> Vec<u8> {
    let mut body = Vec::new();
    // Meta (data_version 0)
    let mut meta = Vec::new();
    meta.extend(m.meta.feature_level.to_le_bytes());
    meta.push(0); // is_file_data
    meta.extend(0u32.to_le_bytes()); // app_id
    for s in [&m.meta.app_name, &m.meta.build_version, &m.meta.launch_exe, &m.meta.launch_command] {
        fstring(&mut meta, s);
    }
    meta.extend(0u32.to_le_bytes()); // prereq ids
    for _ in 0..3 {
        fstring(&mut meta, ""); // prereq name, path, args
    }
    section(&mut body, 0, &meta, opts.meta_padding);
    // Chunk list
    let mut cdl = (m.chunks.len() as u32).to_le_bytes().to_vec();
    m.chunks.iter().for_each(|c| guid(&mut cdl, &c.guid));
    m.chunks.iter().for_each(|c| cdl.extend(c.hash.to_le_bytes()));
    m.chunks.iter().for_each(|c| cdl.extend(c.sha1));
    m.chunks.iter().for_each(|c| cdl.push(c.group));
    m.chunks.iter().for_each(|c| cdl.extend(c.window_size.to_le_bytes()));
    m.chunks.iter().for_each(|c| cdl.extend((c.file_size as i64).to_le_bytes()));
    if m.meta.feature_level >= 22 {
        // Version 22+ columns (Legendary CDL): secret guid, compressed window size, encryption tag.
        for (i, _) in m.chunks.iter().enumerate() {
            cdl.extend(if opts.secret_chunk && i == 0 { [7u8; 16] } else { [0u8; 16] });
        }
        m.chunks.iter().for_each(|c| cdl.extend(c.window_size.to_le_bytes()));
        m.chunks.iter().for_each(|_| cdl.extend([0u8; 16]));
    }
    section(&mut body, 0, &cdl, 0);
    // File list
    let mut fml = (m.files.len() as u32).to_le_bytes().to_vec();
    m.files.iter().for_each(|f| fstring(&mut fml, &f.filename));
    m.files.iter().for_each(|_| fstring(&mut fml, ""));
    m.files.iter().for_each(|f| fml.extend(f.sha1));
    m.files.iter().for_each(|f| fml.push(if f.executable { 0x4 } else { 0 }));
    for f in &m.files {
        fml.extend((f.install_tags.len() as u32).to_le_bytes());
        f.install_tags.iter().for_each(|t| fstring(&mut fml, t));
    }
    for f in &m.files {
        fml.extend((f.parts.len() as u32).to_le_bytes());
        for p in &f.parts {
            fml.extend(28u32.to_le_bytes());
            guid(&mut fml, &p.guid);
            fml.extend(p.offset.to_le_bytes());
            fml.extend(p.size.to_le_bytes());
        }
    }
    section(&mut body, 0, &fml, 0);
    // Custom fields (none)
    section(&mut body, 0, &0u32.to_le_bytes(), 0);

    let sha: [u8; 20] = Sha1::digest(&body).into();
    let stored = if opts.compress {
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(&body).unwrap();
        z.finish().unwrap()
    } else {
        body.clone()
    };
    // Version 22+ headers carry a 16-byte secret guid and a 16-byte tag after the version.
    let v22 = m.meta.feature_level >= 22;
    let mut out = Vec::new();
    out.extend(opts.magic.unwrap_or(0x44BE_C00C).to_le_bytes());
    out.extend((if v22 { 73u32 } else { 41u32 }).to_le_bytes());
    out.extend((body.len() as u32).to_le_bytes());
    out.extend((stored.len() as u32).to_le_bytes());
    out.extend(sha);
    out.push(opts.stored_as.unwrap_or(if opts.compress { 1 } else { 0 }));
    out.extend((if v22 { m.meta.feature_level } else { 18 }).to_le_bytes());
    if v22 {
        out.extend([0u8; 32]);
    }
    out.extend(stored);
    out
}

/// A small manifest used across install tests: 2 chunks, 3 files (one split over both chunks, one empty).
pub fn sample() -> Manifest {
    use super::{ChunkInfo, ChunkPart, FileEntry, ManifestMeta};
    let c1 = [1, 2, 3, 4];
    let c2 = [5, 6, 7, 8];
    Manifest::new(
        ManifestMeta {
            feature_level: 18,
            app_name: "Tidepool".into(),
            build_version: "1.0.0-CL1".into(),
            launch_exe: "Binaries/Tidepool.exe".into(),
            launch_command: "-fish".into(),
        },
        vec![
            ChunkInfo { guid: c1, hash: 0x1A2B, sha1: [1; 20], group: 7, window_size: 1024, file_size: 600 },
            ChunkInfo { guid: c2, hash: 0x3C4D, sha1: [2; 20], group: 42, window_size: 512, file_size: 300 },
        ],
        vec![
            FileEntry { install_tags: vec![],
                filename: "Binaries/Tidepool.exe".into(),
                sha1: [3; 20],
                executable: true,
                parts: vec![ChunkPart { guid: c1, offset: 0, size: 1000 }, ChunkPart { guid: c2, offset: 0, size: 200 }],
            },
            FileEntry { install_tags: vec![], filename: "readme.txt".into(), sha1: [4; 20], executable: false, parts: vec![ChunkPart { guid: c1, offset: 1000, size: 24 }] },
            FileEntry { install_tags: vec![], filename: "empty.cfg".into(), sha1: [5; 20], executable: false, parts: vec![] },
        ],
    )
}
