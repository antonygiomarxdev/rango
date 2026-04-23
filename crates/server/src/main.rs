use std::sync::Arc;

use rango_oplog::FileOplog;
use rango_server::{app, routes::ServerState};
use tracing::info;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let port = std::env::var("RANGO_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8080);

    let token = std::env::var("RANGO_TOKEN").unwrap_or_else(|_| "dev-token".to_string());

    let oplog_path =
        std::env::var("RANGO_OPLOG_PATH").unwrap_or_else(|_| "server-oplog.rgo".to_string());

    let oplog = Arc::new(FileOplog::new(&oplog_path).expect("Failed to open oplog"));
    let state = Arc::new(ServerState::new(oplog));
    state.add_token(&token, "default-node");

    let router = app(state);
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .expect("Failed to bind");

    info!("Rango server listening on port {}", port);
    axum::serve(listener, router).await.expect("Server failed");
}
