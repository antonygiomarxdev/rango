use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Arc;

use rango_storage::CryptoEngine;
use rango_types::{Checkpoint, RangoError};

/// Persistent checkpoint store.
pub trait CheckpointStore: Send + Sync {
    fn get(&self) -> Result<Checkpoint, RangoError>;
    fn set(&self, checkpoint: Checkpoint) -> Result<(), RangoError>;
}

/// File-based checkpoint store (single-line JSON, optionally encrypted).
#[derive(Debug, Clone)]
pub struct FileCheckpointStore {
    path: PathBuf,
    crypto: Option<Arc<CryptoEngine>>,
}

impl FileCheckpointStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self::new_with_crypto(path, None)
    }

    pub fn new_with_crypto(path: impl Into<PathBuf>, crypto: Option<Arc<CryptoEngine>>) -> Self {
        Self {
            path: path.into(),
            crypto,
        }
    }
}

impl CheckpointStore for FileCheckpointStore {
    fn get(&self) -> Result<Checkpoint, RangoError> {
        if !self.path.exists() {
            return Ok(Checkpoint::initial());
        }
        let mut file = File::open(&self.path)
            .map_err(|e| RangoError::Storage(e.to_string()))?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)
            .map_err(|e| RangoError::Storage(e.to_string()))?;

        let plaintext = if let Some(c) = &self.crypto {
            c.decrypt(&buf)?
        } else {
            buf
        };

        let contents = String::from_utf8(plaintext)
            .map_err(|e| RangoError::Storage(format!("invalid UTF-8: {}", e)))?;
        let val: serde_json::Value = serde_json::from_str(&contents)
            .map_err(|e: serde_json::Error| RangoError::Storage(e.to_string()))?;
        let seq = val.get("last_seq")
            .and_then(|v: &serde_json::Value| v.as_u64())
            .unwrap_or(0);
        Ok(Checkpoint(seq))
    }

    fn set(&self, checkpoint: Checkpoint) -> Result<(), RangoError> {
        let val = serde_json::json!({ "last_seq": checkpoint.0 });
        let mut bytes = val.to_string().into_bytes();
        if let Some(c) = &self.crypto {
            bytes = c.encrypt(&bytes);
        }
        let mut file = File::create(&self.path)
            .map_err(|e| RangoError::Storage(e.to_string()))?;
        file.write_all(&bytes)
            .map_err(|e| RangoError::Storage(e.to_string()))?;
        file.sync_all()
            .map_err(|e| RangoError::Storage(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_path() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        std::env::temp_dir().join(format!("rango-checkpoint-test-{}-{}.json", pid, n))
    }

    #[test]
    fn test_get_initial() {
        let path = temp_path();
        let store = FileCheckpointStore::new(&path);
        assert_eq!(store.get().unwrap(), Checkpoint::initial());
    }

    #[test]
    fn test_set_and_get() {
        let path = temp_path();
        let store = FileCheckpointStore::new(&path);
        store.set(Checkpoint(42)).unwrap();
        assert_eq!(store.get().unwrap(), Checkpoint(42));
    }

    #[test]
    fn test_persistence() {
        let path = temp_path();
        {
            let store = FileCheckpointStore::new(&path);
            store.set(Checkpoint(99)).unwrap();
        }
        {
            let store = FileCheckpointStore::new(&path);
            assert_eq!(store.get().unwrap(), Checkpoint(99));
        }
    }
}
