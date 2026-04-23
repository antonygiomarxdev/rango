use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use bson::Document;
use rango_storage::CryptoEngine;
use rango_types::{RangoError, SyncQueueEntry, QueueState};

/// Persistent sync queue with transitional states.
pub trait SyncQueue: Send + Sync {
    fn enqueue(&self, seq: u64) -> Result<(), RangoError>;
    fn next_batch(&self, limit: usize) -> Result<Vec<SyncQueueEntry>, RangoError>;
    fn mark_inflight(&self, seqs: &[u64]) -> Result<(), RangoError>;
    fn mark_acked(&self, seqs: &[u64]) -> Result<(), RangoError>;
    fn mark_failed(&self, seq: u64, error: String) -> Result<(), RangoError>;
}

/// File-based persistent sync queue.
#[derive(Debug, Clone)]
pub struct FileSyncQueue {
    path: PathBuf,
    state: Arc<Mutex<QueueStateMem>>,
    crypto: Option<Arc<CryptoEngine>>,
}

#[derive(Debug)]
struct QueueStateMem {
    entries: HashMap<u64, SyncQueueEntry>,
}

impl FileSyncQueue {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, RangoError> {
        Self::new_with_crypto(path, None)
    }

    pub fn new_with_crypto(
        path: impl Into<PathBuf>,
        crypto: Option<Arc<CryptoEngine>>,
    ) -> Result<Self, RangoError> {
        let path = path.into();
        let entries = Self::read_all(&path, crypto.as_deref())?;
        Ok(Self {
            path,
            state: Arc::new(Mutex::new(QueueStateMem { entries })),
            crypto,
        })
    }

    fn read_all(
        path: &PathBuf,
        crypto: Option<&CryptoEngine>,
    ) -> Result<HashMap<u64, SyncQueueEntry>, RangoError> {
        if !path.exists() {
            return Ok(HashMap::new());
        }
        let file = File::open(path).map_err(|e| RangoError::Storage(e.to_string()))?;
        let mut reader = BufReader::new(file);
        let mut entries = HashMap::new();

        while let Ok(len) = read_u32(&mut reader) {
            let mut buf = vec![0u8; len as usize];
            if reader.read_exact(&mut buf).is_err() {
                break;
            }
            let plaintext = if let Some(c) = crypto {
                match c.decrypt(&buf) {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::warn!("failed to decrypt sync queue entry: {}", e);
                        continue;
                    }
                }
            } else {
                buf
            };
            if let Ok(doc) = Document::from_reader(&mut plaintext.as_slice()) {
                if let Ok(entry) = bson::de::deserialize_from_document::<SyncQueueEntry>(doc) {
                    entries.insert(entry.seq, entry);
                }
            }
        }
        Ok(entries)
    }

    fn write_all(&self, mem: &QueueStateMem) -> Result<(), RangoError> {
        let mut file = File::create(&self.path)
            .map_err(|e| RangoError::Storage(e.to_string()))?;
        for entry in mem.entries.values() {
            let doc = bson::ser::serialize_to_document(entry)
                .map_err(|e: bson::error::Error| RangoError::Storage(e.to_string()))?;
            let mut bytes = Vec::new();
            doc.to_writer(&mut bytes)
                .map_err(|e: bson::error::Error| RangoError::Storage(e.to_string()))?;
            if let Some(c) = &self.crypto {
                bytes = c.encrypt(&bytes);
            }
            write_u32(&mut file, bytes.len() as u32)?;
            file.write_all(&bytes)
                .map_err(|e| RangoError::Storage(e.to_string()))?;
        }
        file.sync_all()
            .map_err(|e| RangoError::Storage(e.to_string()))?;
        Ok(())
    }
}

impl SyncQueue for FileSyncQueue {
    fn enqueue(&self, seq: u64) -> Result<(), RangoError> {
        let mut mem = self.state.lock().map_err(|e| RangoError::Storage(e.to_string()))?;
        if !mem.entries.contains_key(&seq) {
            mem.entries.insert(seq, SyncQueueEntry {
                seq,
                state: QueueState::Pending,
                retries: 0,
                last_error: None,
            });
            self.write_all(&mem)?;
        }
        Ok(())
    }

    fn next_batch(&self, limit: usize) -> Result<Vec<SyncQueueEntry>, RangoError> {
        let mut mem = self.state.lock().map_err(|e| RangoError::Storage(e.to_string()))?;
        let mut batch: Vec<_> = mem.entries
            .values()
            .filter(|e| matches!(e.state, QueueState::Pending))
            .cloned()
            .take(limit)
            .collect();

        for entry in &mut batch {
            entry.state = QueueState::Inflight;
            mem.entries.insert(entry.seq, entry.clone());
        }
        if !batch.is_empty() {
            self.write_all(&mem)?;
        }
        Ok(batch)
    }

    fn mark_inflight(&self, seqs: &[u64]) -> Result<(), RangoError> {
        let mut mem = self.state.lock().map_err(|e| RangoError::Storage(e.to_string()))?;
        for seq in seqs {
            if let Some(entry) = mem.entries.get_mut(seq) {
                entry.state = QueueState::Inflight;
            }
        }
        self.write_all(&mem)?;
        Ok(())
    }

    fn mark_acked(&self, seqs: &[u64]) -> Result<(), RangoError> {
        let mut mem = self.state.lock().map_err(|e| RangoError::Storage(e.to_string()))?;
        for seq in seqs {
            if let Some(entry) = mem.entries.get_mut(seq) {
                entry.state = QueueState::Acked;
            }
        }
        self.write_all(&mem)?;
        Ok(())
    }

    fn mark_failed(&self, seq: u64, error: String) -> Result<(), RangoError> {
        let mut mem = self.state.lock().map_err(|e| RangoError::Storage(e.to_string()))?;
        if let Some(entry) = mem.entries.get_mut(&seq) {
            entry.state = QueueState::Failed;
            entry.retries += 1;
            entry.last_error = Some(error);
        }
        self.write_all(&mem)?;
        Ok(())
    }
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
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_path() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        std::env::temp_dir().join(format!("rango-sync-queue-test-{}-{}.rgo", pid, n))
    }

    #[test]
    fn test_enqueue_and_next_batch() {
        let path = temp_path();
        let queue = FileSyncQueue::new(&path).unwrap();

        queue.enqueue(1).unwrap();
        queue.enqueue(2).unwrap();
        queue.enqueue(3).unwrap();

        let batch = queue.next_batch(10).unwrap();
        assert_eq!(batch.len(), 3);
        assert!(batch.iter().all(|e| matches!(e.state, QueueState::Inflight)));
    }

    #[test]
    fn test_next_batch_respects_limit() {
        let path = temp_path();
        let queue = FileSyncQueue::new(&path).unwrap();

        for i in 1..=5 {
            queue.enqueue(i).unwrap();
        }

        let batch = queue.next_batch(2).unwrap();
        assert_eq!(batch.len(), 2);
    }

    #[test]
    fn test_mark_acked() {
        let path = temp_path();
        let queue = FileSyncQueue::new(&path).unwrap();

        queue.enqueue(1).unwrap();
        let batch = queue.next_batch(10).unwrap();
        assert_eq!(batch.len(), 1);

        queue.mark_acked(&[1]).unwrap();
        let next = queue.next_batch(10).unwrap();
        assert_eq!(next.len(), 0);
    }

    #[test]
    fn test_mark_failed() {
        let path = temp_path();
        let queue = FileSyncQueue::new(&path).unwrap();

        queue.enqueue(1).unwrap();
        queue.next_batch(10).unwrap();
        queue.mark_failed(1, "network error".to_string()).unwrap();

        let next = queue.next_batch(10).unwrap();
        assert_eq!(next.len(), 0); // Failed entries are not retried automatically
    }

    #[test]
    fn test_persistence() {
        let path = temp_path();
        {
            let queue = FileSyncQueue::new(&path).unwrap();
            queue.enqueue(1).unwrap();
            queue.enqueue(2).unwrap();
            queue.next_batch(10).unwrap();
        }
        {
            let queue = FileSyncQueue::new(&path).unwrap();
            let next = queue.next_batch(10).unwrap();
            assert_eq!(next.len(), 0); // Already inflight from previous session
        }
    }
}
