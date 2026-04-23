pub mod routes;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use std::sync::Arc;
use std::time::Duration;
use tower::limit::RateLimitLayer;

use routes::ServerState;

/// Maximum request body size (10 MB) to prevent DoS via large JSON payloads.
const MAX_REQUEST_SIZE: usize = 10 * 1024 * 1024;

/// Rate limit: 100 requests per second with burst of 200.
const RATE_LIMIT_PER_SECOND: u64 = 100;
const RATE_LIMIT_BURST: u32 = 200;

pub fn app(state: Arc<ServerState>) -> Router {
    let rate_limit = RateLimitLayer::new(
        RATE_LIMIT_BURST,
        Duration::from_secs(1),
    );

    Router::new()
        .route("/push", axum::routing::post(routes::handle_push))
        .route("/pull", axum::routing::post(routes::handle_pull))
        .layer(DefaultBodyLimit::max(MAX_REQUEST_SIZE))
        .layer(rate_limit)
        .layer(axum::Extension(state))
}
