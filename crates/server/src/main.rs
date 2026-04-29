use std::sync::Arc;

use clap::Parser;
use rango_oplog::FileOplog;
use rango_server::{app, routes::ServerState};
use tracing::info;

#[derive(Parser, Debug)]
#[command(name = "rango-server")]
#[command(about = "Rango sync hub for memory workspaces")]
struct Cli {
    /// Bind address
    #[arg(long, default_value = "0.0.0.0")]
    bind: String,
    /// TCP port
    #[arg(long, default_value_t = 8080)]
    port: u16,
    /// Auth token accepted for push/pull requests
    #[arg(long)]
    token: Option<String>,
    /// Path to the server oplog file
    #[arg(long, default_value = "server-oplog.rgo")]
    oplog_path: String,
}

fn init_tracing() {
    let log_level = std::env::var("RANGO_LOG_LEVEL")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(tracing::Level::INFO);

    let log_format = std::env::var("RANGO_LOG_FORMAT").unwrap_or_default();

    if log_format == "json" {
        tracing_subscriber::fmt()
            .with_max_level(log_level)
            .json()
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_max_level(log_level)
            .pretty()
            .init();
    }
}

#[tokio::main]
async fn main() {
    init_tracing();
    let cli = Cli::parse();

    let port = std::env::var("RANGO_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(cli.port);

    let token = cli
        .token
        .or_else(|| std::env::var("RANGO_TOKEN").ok())
        .unwrap_or_else(|| "dev-token".to_string());

    let oplog_path = std::env::var("RANGO_OPLOG_PATH").unwrap_or(cli.oplog_path);
    let bind = std::env::var("RANGO_BIND").unwrap_or(cli.bind);

    let oplog = Arc::new(FileOplog::new(&oplog_path).expect("Failed to open oplog"));
    let state = Arc::new(ServerState::new(oplog));
    state.add_token(&token, "default-node");

    let router = app(state);
    let listener = tokio::net::TcpListener::bind(format!("{}:{}", bind, port))
        .await
        .expect("Failed to bind");

    info!(
        "Rango server listening on {}:{} (oplog: {})",
        bind, port, oplog_path
    );
    axum::serve(listener, router).await.expect("Server failed");
}
