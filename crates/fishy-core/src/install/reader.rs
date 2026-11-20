//! Little-endian reader for Epic's binary formats. Reading past the end is `BadManifest("truncated")`.
use super::{Guid, InstallError};

pub(crate) struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
    pub fn pos(&self) -> usize {
        self.pos
    }
    pub fn seek(&mut self, pos: usize) {
        self.pos = pos;
    }
    /// Bytes left after the current position (0 if we've seeked past the end).
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8], InstallError> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.data.len()).ok_or_else(truncated)?;
        let out = &self.data[self.pos..end];
        self.pos = end;
        Ok(out)
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N], InstallError> {
        Ok(self.bytes(N)?.try_into().expect("length checked"))
    }
    pub fn u8(&mut self) -> Result<u8, InstallError> {
        Ok(self.array::<1>()?[0])
    }
    pub fn u32(&mut self) -> Result<u32, InstallError> {
        Ok(u32::from_le_bytes(self.array()?))
    }
    pub fn i32(&mut self) -> Result<i32, InstallError> {
        Ok(i32::from_le_bytes(self.array()?))
    }
    pub fn u64(&mut self) -> Result<u64, InstallError> {
        Ok(u64::from_le_bytes(self.array()?))
    }
    pub fn i64(&mut self) -> Result<i64, InstallError> {
        Ok(i64::from_le_bytes(self.array()?))
    }
    pub fn guid(&mut self) -> Result<Guid, InstallError> {
        Ok([self.u32()?, self.u32()?, self.u32()?, self.u32()?])
    }
    /// Unreal's FString: i32 length incl. the NUL; negative means UTF-16LE code units.
    pub fn fstring(&mut self) -> Result<String, InstallError> {
        let len = self.i32()?;
        if len == 0 {
            return Ok(String::new());
        }
        if len > 0 {
            let raw = self.bytes(len as usize)?;
            let text = raw.strip_suffix(&[0]).unwrap_or(raw);
            return Ok(String::from_utf8_lossy(text).into_owned());
        }
        let units = len.unsigned_abs() as usize;
        let raw = self.bytes(units.checked_mul(2).ok_or_else(truncated)?)?;
        let mut words: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        if words.last() == Some(&0) {
            words.pop();
        }
        Ok(String::from_utf16_lossy(&words))
    }
}

fn truncated() -> InstallError {
    InstallError::BadManifest("truncated".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_le_numbers() {
        let mut r = Reader::new(&[1, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0xFF]);
        assert_eq!(r.u32().unwrap(), 1);
        assert_eq!(r.u64().unwrap(), 2);
        assert_eq!(r.u8().unwrap(), 0xFF);
        assert_eq!(r.pos(), 13);
    }

    #[test]
    fn reads_guid_as_four_le_words() {
        let mut r = Reader::new(&[1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0x80]);
        assert_eq!(r.guid().unwrap(), [1, 2, 3, 0x8000_0000]);
    }

    #[test]
    fn fstring_ascii_drops_nul() {
        let mut r = Reader::new(&[3, 0, 0, 0, b'h', b'i', 0]);
        assert_eq!(r.fstring().unwrap(), "hi");
        assert_eq!(r.pos(), 7);
    }

    #[test]
    fn fstring_utf16() {
        let mut bytes = (-3i32).to_le_bytes().to_vec();
        for unit in ['é' as u16, 'x' as u16, 0] {
            bytes.extend(unit.to_le_bytes());
        }
        let mut r = Reader::new(&bytes);
        assert_eq!(r.fstring().unwrap(), "éx");
        assert_eq!(r.pos(), bytes.len());
    }

    #[test]
    fn fstring_empty() {
        let mut r = Reader::new(&[0, 0, 0, 0]);
        assert_eq!(r.fstring().unwrap(), "");
    }

    #[test]
    fn truncated_is_bad_manifest() {
        let mut r = Reader::new(&[1, 2]);
        assert_eq!(r.u32(), Err(InstallError::BadManifest("truncated".into())));
        let mut r = Reader::new(&[10, 0, 0, 0, b'a']);
        assert_eq!(r.fstring(), Err(InstallError::BadManifest("truncated".into())));
    }
}
