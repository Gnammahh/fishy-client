use super::Session;
use std::collections::BTreeMap;
use std::sync::Mutex;

/// Windows Credential Manager entry names (spec §6a).
pub const SERVICE: &str = "FishyClient";
pub const ACCOUNT: &str = "epic-session";
/// Windows caps one credential at 2560 bytes; stay well under it.
pub(crate) const CHUNK_BYTES: usize = 2000;

/// Why the saved session couldn't be read or written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    /// The saved data is unreadable (crash mid-save, edited by hand). Safe to delete.
    Damaged(String),
    /// Credential Manager itself failed (locked, busy, access denied). The data may be fine.
    Backend(String),
}

/// Where the signed-in session is kept between app starts.
pub trait SessionStore: Send + Sync {
    fn load(&self) -> Result<Option<Session>, StoreError>;
    fn save(&self, session: &Session) -> Result<(), StoreError>;
    fn clear(&self) -> Result<(), StoreError>;
}

/// A tiny key -> bytes secret store (Windows Credential Manager in the app, memory in tests).
pub trait CredentialBackend: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, String>;
    fn set(&self, key: &str, value: &[u8]) -> Result<(), String>;
    /// Deleting a key that doesn't exist is fine.
    fn delete(&self, key: &str) -> Result<(), String>;
}

/// Stores the session JSON split into <=2000-byte chunks:
/// `epic-session` holds the chunk count, `epic-session.0`, `.1`, ... hold the data.
pub struct ChunkedStore<B: CredentialBackend> {
    backend: B,
}

impl<B: CredentialBackend> ChunkedStore<B> {
    pub fn new(backend: B) -> Self {
        Self { backend }
    }
    pub fn backend(&self) -> &B {
        &self.backend
    }
    fn chunk_key(i: usize) -> String {
        format!("{ACCOUNT}.{i}")
    }
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        self.backend.get(key).map_err(StoreError::Backend)
    }
    fn set(&self, key: &str, value: &[u8]) -> Result<(), StoreError> {
        self.backend.set(key, value).map_err(StoreError::Backend)
    }
    fn delete(&self, key: &str) -> Result<(), StoreError> {
        self.backend.delete(key).map_err(StoreError::Backend)
    }
    fn count(&self) -> Result<Option<usize>, StoreError> {
        match self.get(ACCOUNT)? {
            None => Ok(None),
            Some(bytes) => String::from_utf8(bytes)
                .ok()
                .and_then(|s| s.trim().parse().ok())
                .map(Some)
                .ok_or_else(|| StoreError::Damaged("saved session header is damaged".into())),
        }
    }
}

impl<B: CredentialBackend> SessionStore for ChunkedStore<B> {
    fn load(&self) -> Result<Option<Session>, StoreError> {
        let Some(count) = self.count()? else { return Ok(None) };
        let mut json = Vec::new();
        for i in 0..count {
            let part = self
                .get(&Self::chunk_key(i))?
                .ok_or_else(|| StoreError::Damaged("saved session is incomplete".into()))?;
            json.extend(part);
        }
        serde_json::from_slice(&json)
            .map(Some)
            .map_err(|_| StoreError::Damaged("saved session is damaged".into()))
    }

    fn save(&self, session: &Session) -> Result<(), StoreError> {
        let json = serde_json::to_vec(session).map_err(|e| StoreError::Damaged(e.to_string()))?;
        let old = self.count().unwrap_or(None).unwrap_or(0);
        let chunks: Vec<&[u8]> = json.chunks(CHUNK_BYTES).collect();
        for (i, chunk) in chunks.iter().enumerate() {
            self.set(&Self::chunk_key(i), chunk)?;
        }
        self.set(ACCOUNT, chunks.len().to_string().as_bytes())?;
        for i in chunks.len()..old {
            self.delete(&Self::chunk_key(i))?;
        }
        Ok(())
    }

    /// Deletes every chunk, even ones a damaged or missing header no longer points to.
    fn clear(&self) -> Result<(), StoreError> {
        let known = self.count().unwrap_or(None).unwrap_or(0);
        let mut i = 0;
        loop {
            let key = Self::chunk_key(i);
            if i >= known && self.get(&key)?.is_none() {
                break;
            }
            self.delete(&key)?;
            i += 1;
        }
        self.delete(ACCOUNT)
    }
}

/// Windows Credential Manager via the `keyring` crate.
pub struct KeyringBackend {
    service: String,
}

impl KeyringBackend {
    pub fn new(service: &str) -> Self {
        Self { service: service.to_string() }
    }
    fn entry(&self, key: &str) -> Result<keyring::Entry, String> {
        keyring::Entry::new(&self.service, key).map_err(|e| e.to_string())
    }
}

impl CredentialBackend for KeyringBackend {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, String> {
        match self.entry(key)?.get_secret() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
    fn set(&self, key: &str, value: &[u8]) -> Result<(), String> {
        self.entry(key)?.set_secret(value).map_err(|e| e.to_string())
    }
    fn delete(&self, key: &str) -> Result<(), String> {
        match self.entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// The real store used by the app.
pub type KeyringStore = ChunkedStore<KeyringBackend>;

impl ChunkedStore<KeyringBackend> {
    pub fn windows() -> Self {
        Self::new(KeyringBackend::new(SERVICE))
    }
}

/// In-memory backend for tests.
#[derive(Default)]
pub struct MemoryBackend(Mutex<BTreeMap<String, Vec<u8>>>);

impl MemoryBackend {
    pub fn entries(&self) -> BTreeMap<String, Vec<u8>> {
        self.0.lock().unwrap().clone()
    }
}

impl CredentialBackend for MemoryBackend {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(self.0.lock().unwrap().get(key).cloned())
    }
    fn set(&self, key: &str, value: &[u8]) -> Result<(), String> {
        self.0.lock().unwrap().insert(key.to_string(), value.to_vec());
        Ok(())
    }
    fn delete(&self, key: &str) -> Result<(), String> {
        self.0.lock().unwrap().remove(key);
        Ok(())
    }
}

/// Simple in-memory session store for tests.
#[derive(Default)]
pub struct MemoryStore(Mutex<Option<Session>>);

impl MemoryStore {
    pub fn with(session: Session) -> Self {
        Self(Mutex::new(Some(session)))
    }
}

impl SessionStore for MemoryStore {
    fn load(&self) -> Result<Option<Session>, StoreError> {
        Ok(self.0.lock().unwrap().clone())
    }
    fn save(&self, session: &Session) -> Result<(), StoreError> {
        *self.0.lock().unwrap() = Some(session.clone());
        Ok(())
    }
    fn clear(&self) -> Result<(), StoreError> {
        *self.0.lock().unwrap() = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::Secret;
    use chrono::{TimeZone, Utc};

    fn big_session() -> Session {
        Session {
            access_token: Secret::new("A".repeat(5000)),
            refresh_token: Secret::new("R".repeat(5000)),
            expires_at: Utc.with_ymd_and_hms(2026, 11, 20, 20, 0, 0).unwrap(),
            refresh_expires_at: Utc.with_ymd_and_hms(2026, 12, 13, 18, 0, 0).unwrap(),
            account_id: "acc123".into(),
            display_name: "FishFan".into(),
        }
    }

    #[test]
    fn missing_entry_loads_none() {
        let store = ChunkedStore::new(MemoryBackend::default());
        assert_eq!(store.load().unwrap(), None);
    }

    #[test]
    fn large_session_round_trips_in_chunks() {
        let store = ChunkedStore::new(MemoryBackend::default());
        store.save(&big_session()).unwrap();
        let entries = store.backend().entries();
        assert!(entries.len() > 3, "expected several chunks, got {}", entries.len());
        assert!(entries.values().all(|v| v.len() <= CHUNK_BYTES));
        assert_eq!(store.load().unwrap(), Some(big_session()));
    }

    #[test]
    fn clear_removes_all_chunks() {
        let store = ChunkedStore::new(MemoryBackend::default());
        store.save(&big_session()).unwrap();
        store.clear().unwrap();
        assert!(store.backend().entries().is_empty());
        assert_eq!(store.load().unwrap(), None);
    }

    #[test]
    fn saving_a_smaller_session_removes_leftover_chunks() {
        let store = ChunkedStore::new(MemoryBackend::default());
        store.save(&big_session()).unwrap();
        let mut small = big_session();
        small.access_token = Secret::new("a");
        small.refresh_token = Secret::new("r");
        store.save(&small).unwrap();
        assert_eq!(store.backend().entries().len(), 2); // header + 1 chunk
        assert_eq!(store.load().unwrap(), Some(small));
    }

    /// Writes to the real Windows Credential Manager under a throwaway service name.
    /// Run with: cargo test -p fishy-core -- --ignored real_credential_manager
    #[test]
    #[ignore]
    fn real_credential_manager_round_trips_large_session() {
        let store = ChunkedStore::new(KeyringBackend::new("FishyClient-test"));
        store.save(&big_session()).unwrap();
        assert_eq!(store.load().unwrap(), Some(big_session()));
        store.clear().unwrap();
        assert_eq!(store.load().unwrap(), None);
    }

    #[test]
    fn clear_removes_orphan_chunks_without_header() {
        let store = ChunkedStore::new(MemoryBackend::default());
        // a save that crashed before writing the header
        store.backend().set("epic-session.0", b"{\"access_token\":").unwrap();
        store.backend().set("epic-session.1", b"\"eg1~SECRET\"").unwrap();
        store.clear().unwrap();
        assert!(store.backend().entries().is_empty());
    }

    #[test]
    fn clear_removes_chunks_when_header_damaged() {
        let store = ChunkedStore::new(MemoryBackend::default());
        store.save(&big_session()).unwrap();
        store.backend().set("epic-session", b"garbage").unwrap();
        assert!(matches!(store.load(), Err(StoreError::Damaged(_))));
        store.clear().unwrap();
        assert!(store.backend().entries().is_empty());
    }

    #[test]
    fn memory_store_round_trips() {
        let store = MemoryStore::default();
        store.save(&big_session()).unwrap();
        assert_eq!(store.load().unwrap(), Some(big_session()));
        store.clear().unwrap();
        assert_eq!(store.load().unwrap(), None);
    }
}
