pub mod routes;
pub mod retrieval;

use axum::{Router, extract::DefaultBodyLimit, middleware};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use routes::ServerState;

/// Maximum request body size (10 MB) to prevent DoS via large JSON payloads.
const MAX_REQUEST_SIZE: usize = 10 * 1024 * 1024;

/// Rate limit: 100 requests per second globally, with burst of 200.
const RATE_LIMIT_BURST: u32 = 200;

#[derive(Debug, Clone)]
struct RateLimiter {
    state: Arc<Mutex<(Instant, u32)>>,
    burst: u32,
}

impl RateLimiter {
    fn new(burst: u32) -> Self {
        Self {
            state: Arc::new(Mutex::new((Instant::now(), 0))),
            burst,
        }
    }

    fn check(&self) -> bool {
        let mut state = self.state.lock().unwrap();
        let now = Instant::now();
        let window = Duration::from_secs(1);

        if now.duration_since(state.0) >= window {
            state.0 = now;
            state.1 = 0;
        }

        if state.1 < self.burst {
            state.1 += 1;
            true
        } else {
            false
        }
    }
}

async fn rate_limit_middleware(
    request: axum::extract::Request,
    next: middleware::Next,
) -> axum::response::Response {
    static LIMITER: std::sync::OnceLock<RateLimiter> = std::sync::OnceLock::new();
    let limiter = LIMITER.get_or_init(|| RateLimiter::new(RATE_LIMIT_BURST));

    if !limiter.check() {
        return axum::response::Response::builder()
            .status(axum::http::StatusCode::TOO_MANY_REQUESTS)
            .body(axum::body::Body::empty())
            .unwrap();
    }

    next.run(request).await
}

pub fn app(state: Arc<ServerState>) -> Router {
    Router::new()
        .route("/push", axum::routing::post(routes::handle_push))
        .route("/pull", axum::routing::post(routes::handle_pull))
        .route("/promote", axum::routing::post(routes::handle_promote))
        .route("/retrieve", axum::routing::post(routes::handle_retrieval_read))
        .layer(DefaultBodyLimit::max(MAX_REQUEST_SIZE))
        .layer(middleware::from_fn(rate_limit_middleware))
        .layer(axum::Extension(state))
}
