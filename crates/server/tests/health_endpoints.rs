use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use axum::Extension;
use axum::http::StatusCode;
use rango_oplog::Oplog;
use rango_server::routes::{HealthResponse, ReadyResponse, ServerState, handle_health, handle_ready};
use rango_types::{OplogEntry, RangoError};

#[derive(Default)]
struct InMemoryOplog {
    seq: AtomicU64,
    entries: Mutex<Vec<OplogEntry>>,
}

impl Oplog for InMemoryOplog {
    fn append(&self, mut entry: OplogEntry) -> Result<u64, RangoError> {
        let seq = self.seq.fetch_add(1, Ordering::Relaxed) + 1;
        entry.seq = seq;
        self.entries.lock().unwrap().push(entry);
        Ok(seq)
    }

    fn read_since(&self, seq: u64, limit: usize) -> Result<Vec<OplogEntry>, RangoError> {
        Ok(self
            .entries
            .lock()
            .unwrap()
            .iter()
            .filter(|entry| entry.seq >= seq)
            .take(limit)
            .cloned()
            .collect())
    }

    fn mark_applied(&self, _seq: u64) -> Result<(), RangoError> {
        Ok(())
    }

    fn latest_seq(&self) -> Result<u64, RangoError> {
        Ok(self.seq.load(Ordering::Relaxed))
    }
}

#[derive(Default)]
struct BrokenOplog;

impl Oplog for BrokenOplog {
    fn append(&self, _entry: OplogEntry) -> Result<u64, RangoError> {
        Err(RangoError::Sync("broken".to_string()))
    }

    fn read_since(&self, _seq: u64, _limit: usize) -> Result<Vec<OplogEntry>, RangoError> {
        Err(RangoError::Sync("broken".to_string()))
    }

    fn mark_applied(&self, _seq: u64) -> Result<(), RangoError> {
        Err(RangoError::Sync("broken".to_string()))
    }

    fn latest_seq(&self) -> Result<u64, RangoError> {
        Err(RangoError::Sync("oplog unavailable".to_string()))
    }
}

#[tokio::test]
async fn health_returns_200_and_healthy_json() {
    let (status, axum::Json(body)): (StatusCode, axum::Json<HealthResponse>) = handle_health().await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.status, "healthy");
}

#[tokio::test]
async fn ready_returns_200_when_oplog_accessible() {
    let oplog = Arc::new(InMemoryOplog::default());
    let state = Arc::new(ServerState::new(oplog));
    let (status, axum::Json(body)): (StatusCode, axum::Json<ReadyResponse>) =
        handle_ready(Extension(state)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.status, "ready");
    assert!(body.reason.is_none());
}

#[tokio::test]
async fn ready_returns_503_when_oplog_broken() {
    let oplog = Arc::new(BrokenOplog::default());
    let state = Arc::new(ServerState::new(oplog));
    let (status, axum::Json(body)): (StatusCode, axum::Json<ReadyResponse>) =
        handle_ready(Extension(state)).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body.status, "not_ready");
    assert!(body.reason.is_some());
}
