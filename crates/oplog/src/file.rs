use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use bson::{Document, doc};
use rango_storage::CryptoEngine;
use rango_types::{OplogEntry, RangoError};
use tracing::{info, instrument};

use crate::Oplog;

/// File-based append-only oplog.
/// Entries are stored as `[u32: len][BSON bytes]` (or encrypted bytes if crypto is enabled).
/// Applied markers are tracked in a separate `.applied` file.
#[derive(Debug, Clone)]
pub struct FileOplog {
    path: PathBuf,
    state: Arc<Mutex<FileOplogState>>,
    crypto: Option<Arc<CryptoEngine>>,
}

#[derive(Debug)]
struct FileOplogState {
    next_seq: u64,
    applied: HashSet<u64>,
}

impl FileOplog {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, RangoError> {
        Self::new_with_crypto(path, None)
    }

    pub fn new_with_crypto(
        path: impl Into<PathBuf>,
        crypto: Option<Arc<CryptoEngine>>,
    ) -> Result<Self, RangoError> {
        let path = path.into();
        let applied_path = applied_file_path(&path);

        // Ensure file exists
        if !path.exists() {
            File::create(&path).map_err(|e| RangoError::Storage(e.to_string()))?;
        }

        let next_seq = Self::read_latest_seq(&path, crypto.as_deref())?;
        let applied = Self::read_applied(&applied_path, crypto.as_deref())?;

        Ok(Self {
            path,
            state: Arc::new(Mutex::new(FileOplogState { next_seq, applied })),
            crypto,
        })
    }

    fn read_latest_seq(path: &PathBuf, crypto: Option<&CryptoEngine>) -> Result<u64, RangoError> {
        let file = File::open(path).map_err(|e| RangoError::Storage(e.to_string()))?;
        let mut reader = BufReader::new(file);
        let mut latest = 0u64;

        while let Ok(len) = read_u32(&mut reader) {
            let mut buf = vec![0u8; len as usize];
            if reader.read_exact(&mut buf).is_err() {
                break;
            }
            let plaintext = if let Some(c) = crypto {
                c.decrypt(&buf)?
            } else {
                buf
            };
            if let Ok(doc) = Document::from_reader(&mut plaintext.as_slice()) {
                if let Ok(entry) = bson::de::deserialize_from_document::<OplogEntry>(doc) {
                    latest = entry.seq;
                }
            }
        }
        Ok(latest)
    }

    fn read_applied(
        path: &PathBuf,
        crypto: Option<&CryptoEngine>,
    ) -> Result<HashSet<u64>, RangoError> {
        if !path.exists() {
            return Ok(HashSet::new());
        }
        let file = File::open(path).map_err(|e| RangoError::Storage(e.to_string()))?;
        let mut reader = BufReader::new(file);
        let mut buf = Vec::new();
        if reader.read_to_end(&mut buf).is_err() || buf.is_empty() {
            return Ok(HashSet::new());
        }
        let plaintext = if let Some(c) = crypto {
            c.decrypt(&buf)?
        } else {
            buf
        };
        if let Ok(doc) = Document::from_reader(&mut plaintext.as_slice()) {
            if let Ok(arr) = doc.get_array("applied") {
                let set: HashSet<u64> = arr
                    .iter()
                    .filter_map(|b| b.as_i64().map(|v| v as u64))
                    .collect();
                return Ok(set);
            }
        }
        Ok(HashSet::new())
    }

    fn write_applied(&self, state: &FileOplogState) -> Result<(), RangoError> {
        let applied_path = applied_file_path(&self.path);
        let arr: bson::Array = state
            .applied
            .iter()
            .map(|&v| bson::Bson::Int64(v as i64))
            .collect();
        let doc = doc! { "applied": arr };
        let mut bytes = Vec::new();
        doc.to_writer(&mut bytes)
            .map_err(|e: bson::error::Error| RangoError::Storage(e.to_string()))?;
        if let Some(c) = &self.crypto {
            bytes = c.encrypt(&bytes);
        }
        let mut file =
            File::create(&applied_path).map_err(|e| RangoError::Storage(e.to_string()))?;
        file.write_all(&bytes)
            .map_err(|e| RangoError::Storage(e.to_string()))?;
        file.sync_all()
            .map_err(|e| RangoError::Storage(e.to_string()))?;
        Ok(())
    }
}

impl Oplog for FileOplog {
    #[instrument(skip(self, entry))]
    fn append(&self, mut entry: OplogEntry) -> Result<u64, RangoError> {
        info!(seq = entry.seq, "appending to oplog");
        let mut state = self
            .state
            .lock()
            .map_err(|e| RangoError::Storage(e.to_string()))?;
        state.next_seq += 1;
        entry.seq = state.next_seq;

        let doc = bson::ser::serialize_to_document(&entry)
            .map_err(|e| RangoError::Storage(e.to_string()))?;
        let mut bytes = Vec::new();
        doc.to_writer(&mut bytes)
            .map_err(|e: bson::error::Error| RangoError::Storage(e.to_string()))?;

        if let Some(c) = &self.crypto {
            bytes = c.encrypt(&bytes);
        }

        let mut file = OpenOptions::new()
            .append(true)
            .open(&self.path)
            .map_err(|e| RangoError::Storage(e.to_string()))?;

        write_u32(&mut file, bytes.len() as u32)?;
        file.write_all(&bytes)
            .map_err(|e| RangoError::Storage(e.to_string()))?;
        file.sync_all()
            .map_err(|e| RangoError::Storage(e.to_string()))?;

        Ok(entry.seq)
    }

    #[instrument(skip(self), fields(seq, limit))]
    fn read_since(&self, seq: u64, limit: usize) -> Result<Vec<OplogEntry>, RangoError> {
        let file = File::open(&self.path).map_err(|e| RangoError::Storage(e.to_string()))?;
        let mut reader = BufReader::new(file);
        let mut entries = Vec::new();

        while let Ok(len) = read_u32(&mut reader) {
            let mut buf = vec![0u8; len as usize];
            if reader.read_exact(&mut buf).is_err() {
                break;
            }
            let plaintext = if let Some(c) = &self.crypto {
                match c.decrypt(&buf) {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::warn!("failed to decrypt oplog entry: {}", e);
                        continue;
                    }
                }
            } else {
                buf
            };
            if let Ok(doc) = Document::from_reader(&mut plaintext.as_slice()) {
                if let Ok(entry) = bson::de::deserialize_from_document::<OplogEntry>(doc) {
                    if entry.seq >= seq {
                        entries.push(entry);
                        if entries.len() >= limit {
                            break;
                        }
                    }
                }
            }
        }
        Ok(entries)
    }

    #[instrument(skip(self), fields(seq))]
    fn mark_applied(&self, seq: u64) -> Result<(), RangoError> {
        let mut state = self
            .state
            .lock()
            .map_err(|e| RangoError::Storage(e.to_string()))?;
        state.applied.insert(seq);
        self.write_applied(&state)?;
        Ok(())
    }

    #[instrument(skip(self))]
    fn latest_seq(&self) -> Result<u64, RangoError> {
        let state = self
            .state
            .lock()
            .map_err(|e| RangoError::Storage(e.to_string()))?;
        Ok(state.next_seq)
    }
}

impl FileOplog {
    /// Compact the oplog by removing applied entries.
    /// Writes a new file atomically and swaps it in.
    #[instrument(skip(self))]
    pub fn compact(&self) -> Result<u64, RangoError> {
        let state = self
            .state
            .lock()
            .map_err(|e| RangoError::Storage(e.to_string()))?;
        let file = File::open(&self.path).map_err(|e| RangoError::Storage(e.to_string()))?;
        let mut reader = BufReader::new(file);
        let temp_path = self.path.with_extension("compact.tmp");
        let mut temp_file =
            File::create(&temp_path).map_err(|e| RangoError::Storage(e.to_string()))?;
        let mut kept = 0u64;

        while let Ok(len) = read_u32(&mut reader) {
            let mut buf = vec![0u8; len as usize];
            if reader.read_exact(&mut buf).is_err() {
                break;
            }
            let plaintext = if let Some(c) = &self.crypto {
                match c.decrypt(&buf) {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::warn!("compact: failed to decrypt entry, keeping: {}", e);
                        write_u32(&mut temp_file, len)?;
                        temp_file
                            .write_all(&buf)
                            .map_err(|e| RangoError::Storage(e.to_string()))?;
                        kept += 1;
                        continue;
                    }
                }
            } else {
                buf.clone()
            };
            if let Ok(doc) = Document::from_reader(&mut plaintext.as_slice()) {
                if let Ok(entry) = bson::de::deserialize_from_document::<OplogEntry>(doc.clone()) {
                    if !state.applied.contains(&entry.seq) {
                        write_u32(&mut temp_file, len)?;
                        temp_file
                            .write_all(&buf)
                            .map_err(|e| RangoError::Storage(e.to_string()))?;
                        kept += 1;
                    }
                } else {
                    // If we can't deserialize, keep it to be safe
                    write_u32(&mut temp_file, len)?;
                    temp_file
                        .write_all(&buf)
                        .map_err(|e| RangoError::Storage(e.to_string()))?;
                    kept += 1;
                }
            }
        }

        temp_file
            .sync_all()
            .map_err(|e| RangoError::Storage(e.to_string()))?;
        std::fs::rename(&temp_path, &self.path).map_err(|e| RangoError::Storage(e.to_string()))?;

        info!(kept, "oplog compacted");
        Ok(kept)
    }
}

fn applied_file_path(path: &Path) -> PathBuf {
    let mut p = path.to_path_buf();
    let name = p
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    p.set_file_name(format!("{}.applied", name));
    p
}

fn read_u32<R: Read>(reader: &mut R) -> std::io::Result<u32> {
    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

fn write_u32<W: Write>(writer: &mut W, value: u32) -> Result<(), RangoError> {
    writer
        .write_all(&value.to_le_bytes())
        .map_err(|e| RangoError::Storage(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bson::doc;
    use rango_types::{Mutation, MutationOp, OplogOrigin, Revision};

    fn temp_path() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        std::env::temp_dir().join(format!("rango-oplog-test-{}-{}.rgo", pid, n))
    }

    fn dummy_mutation() -> Mutation {
        let doc_id = rango_types::DocumentId::new_uuid_v7();
        let rev = Revision::now("node-a");
        Mutation {
            op: MutationOp::Insert,
            collection: "test".to_string(),
            doc_id: doc_id.clone(),
            patch: Some(doc! { "name": "Alice" }),
            seq: 0,
            timestamp: bson::DateTime::now(),
            rev: rev.clone(),
            write_id: "test-write-id".to_string(),
            metadata: rango_types::MutationMetadata {
                id: doc_id.clone(),
                namespace: "test".to_string(),
                tenant_id: "tenant-a".to_string(),
                r#type: "state".to_string(),
                rev,
                created_at: bson::DateTime::now(),
                updated_at: bson::DateTime::now(),
                source: "node-a".to_string(),
                actor: "node-a".to_string(),
                lineage: doc_id.to_string(),
                schema_version: 1,
                trust_score: 1.0,
                verified: Some(true),
                expires_at: None,
            },
        }
    }

    #[test]
    fn test_append_and_read() {
        let path = temp_path();
        let oplog = FileOplog::new(&path).unwrap();

        let mut e1 = OplogEntry {
            seq: 0,
            timestamp: bson::DateTime::now(),
            mutation: dummy_mutation(),
            origin: OplogOrigin::Local,
            applied: false,
            snapshot_anchor: None,
        };
        let seq1 = oplog.append(e1.clone()).unwrap();
        assert_eq!(seq1, 1);

        e1.mutation.collection = "test2".to_string();
        let seq2 = oplog.append(e1).unwrap();
        assert_eq!(seq2, 2);

        let entries = oplog.read_since(1, 10).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].seq, 1);
        assert_eq!(entries[1].seq, 2);
    }

    #[test]
    fn test_read_since_with_limit() {
        let path = temp_path();
        let oplog = FileOplog::new(&path).unwrap();

        for _ in 0..5 {
            let e = OplogEntry {
                seq: 0,
                timestamp: bson::DateTime::now(),
                mutation: dummy_mutation(),
                origin: OplogOrigin::Local,
                applied: false,
                snapshot_anchor: None,
            };
            oplog.append(e).unwrap();
        }

        let entries = oplog.read_since(2, 2).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].seq, 2);
        assert_eq!(entries[1].seq, 3);
    }

    #[test]
    fn test_latest_seq() {
        let path = temp_path();
        let oplog = FileOplog::new(&path).unwrap();
        assert_eq!(oplog.latest_seq().unwrap(), 0);

        let e = OplogEntry {
            seq: 0,
            timestamp: bson::DateTime::now(),
            mutation: dummy_mutation(),
            origin: OplogOrigin::Local,
            applied: false,
            snapshot_anchor: None,
        };
        oplog.append(e).unwrap();
        assert_eq!(oplog.latest_seq().unwrap(), 1);
    }

    #[test]
    fn test_mark_applied() {
        let path = temp_path();
        let oplog = FileOplog::new(&path).unwrap();

        let e = OplogEntry {
            seq: 0,
            timestamp: bson::DateTime::now(),
            mutation: dummy_mutation(),
            origin: OplogOrigin::Local,
            applied: false,
            snapshot_anchor: None,
        };
        let seq = oplog.append(e).unwrap();
        oplog.mark_applied(seq).unwrap();

        // Verify persistence by reopening
        let oplog2 = FileOplog::new(&path).unwrap();
        assert_eq!(oplog2.latest_seq().unwrap(), 1);
    }

    #[test]
    fn test_persistence() {
        let path = temp_path();
        {
            let oplog = FileOplog::new(&path).unwrap();
            let e = OplogEntry {
                seq: 0,
                timestamp: bson::DateTime::now(),
                mutation: dummy_mutation(),
                origin: OplogOrigin::Local,
                applied: false,
                snapshot_anchor: None,
            };
            oplog.append(e).unwrap();
        }

        {
            let oplog = FileOplog::new(&path).unwrap();
            assert_eq!(oplog.latest_seq().unwrap(), 1);
            let entries = oplog.read_since(1, 10).unwrap();
            assert_eq!(entries.len(), 1);
        }
    }

    #[test]
    fn test_encrypted_round_trip() {
        let path = temp_path();
        let crypto = Arc::new(CryptoEngine::from_passphrase("secret", b"salt"));

        let oplog = FileOplog::new_with_crypto(&path, Some(crypto.clone())).unwrap();
        let e = OplogEntry {
            seq: 0,
            timestamp: bson::DateTime::now(),
            mutation: dummy_mutation(),
            origin: OplogOrigin::Local,
            applied: false,
            snapshot_anchor: None,
        };
        let seq = oplog.append(e).unwrap();

        let entries = oplog.read_since(seq, 10).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].seq, seq);

        // Verify file is not plaintext BSON
        let raw = std::fs::read(&path).unwrap();
        let plaintext_prefix = &[0x5b, 0x00, 0x00, 0x00]; // BSON doc start
        assert!(!raw.windows(4).any(|w| w == plaintext_prefix));
    }
}
