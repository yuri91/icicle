use axum::{http::StatusCode, response::Json, routing::get, Router};
use serde_json::{json, Value};
use std::{
    net::SocketAddr,
    sync::{atomic::AtomicU64, Arc},
};
use tokio::sync::Mutex;
use tracing::{info, Level};
use tracing_subscriber;

mod build;
mod nix;
mod webhook;

use build::BuildQueue;
use webhook::WebhookConfig;

#[derive(Debug)]
pub struct AppState {
    pub build_queue: Mutex<BuildQueue>,
    pub workflow_counter: AtomicU64,
    pub webhook_config: WebhookConfig,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    // Initialize app state
    let app_state = Arc::new(AppState {
        build_queue: Mutex::new(BuildQueue::new()),
        workflow_counter: AtomicU64::new(0),
        webhook_config: WebhookConfig {
            secret: std::env::var("GITHUB_WEBHOOK_SECRET").ok(),
        },
    });

    let app = Router::new()
        .route("/", get(root))
        .route("/health", get(health))
        .merge(webhook::routes())
        .with_state(app_state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    info!("Starting icicle server on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn root() -> Json<Value> {
    Json(json!({
        "name": "icicle",
        "version": "0.1.0",
        "description": "Nix-based CI builder and dashboard"
    }))
}

async fn health() -> StatusCode {
    StatusCode::OK
}
