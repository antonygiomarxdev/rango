pub mod file;

pub use file::*;

use rango_types::{OplogEntry, RangoError};
use std::sync::atomic::{AtomicU64, Ordering};

/// Append-only operation log.
pub trait Oplog: Send + Sync {
    /// Append an entry and return its assigned sequence number.
    fn append(&self, entry: OplogEntry) -> Result<u64, RangoError>;

    /// Read entries starting from `seq` (inclusive), up to `limit`.
    fn read_since(&self, seq: u64, limit: usize) -> Result<Vec<OplogEntry>, RangoError>;

    /// Mark an entry as applied.
    fn mark_applied(&self, seq: u64) -> Result<(), RangoError>;

    /// Return the latest assigned sequence number.
    fn latest_seq(&self) -> Result<u64, RangoError>;
}

/// No-op oplog for testing and scenarios where persistence is not required.
#[derive(Debug, Default)]
pub struct NullOplog {
    seq: AtomicU64,
}

impl NullOplog {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Oplog for NullOplog {
    fn append(&self, mut entry: OplogEntry) -> Result<u64, RangoError> {
        let seq = self.seq.fetch_add(1, Ordering::Relaxed) + 1;
        entry.seq = seq;
        Ok(seq)
    }

    fn read_since(&self, _seq: u64, _limit: usize) -> Result<Vec<OplogEntry>, RangoError> {
        Ok(Vec::new())
    }

    fn mark_applied(&self, _seq: u64) -> Result<(), RangoError> {
        Ok(())
    }

    fn latest_seq(&self) -> Result<u64, RangoError> {
        Ok(self.seq.load(Ordering::Relaxed))
    }
}
