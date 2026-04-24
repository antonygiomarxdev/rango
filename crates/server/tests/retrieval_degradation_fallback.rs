use std::sync::Arc;

use axum::extract::Json;
use axum::http::{HeaderMap, HeaderValue};
use axum::Extension;
use rango_oplog::NullOplog;
use rango_server::routes::{handle_retrieval_read, ServerState};
use rango_types::{RetrievalCapabilityRequest, RetrievalStatus};

fn headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("X-Rango-Protocol-Version", HeaderValue::from_static("1"));
    headers.insert("Authorization", HeaderValue::from_static("Bearer token-1"));
    headers
}

#[tokio::test]
async fn retrieval_timeout_degrades_to_canonical_safe_response() {
    let state = Arc::new(ServerState::new(Arc::new(NullOplog::new())));
    state.add_token_with_tenant("token-1", "node-1", "tenant-a");

    let response = handle_retrieval_read(
        Extension(state),
        headers(),
        Json(RetrievalCapabilityRequest::new(
            "tenant-a",
            "ns-a",
            "find incidents",
        )),
    )
    .await
    .expect("retrieval route should not hard fail")
    .0;

    assert!(response.canonical_fallback);
    assert!(matches!(response.status, RetrievalStatus::Degraded));
    assert_eq!(response.retrieval_status_reason, "adapter_unavailable");
}
