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
mod cache;
mod dashboard;
mod nix;
mod webhook;

use build::BuildQueue;
use cache::CacheConfig;
use webhook::WebhookConfig;

#[derive(Debug)]
pub struct AppState {
    pub build_queue: Mutex<BuildQueue>,
    pub workflow_counter: AtomicU64,
    pub webhook_config: WebhookConfig,
    pub cache_config: CacheConfig,
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
        cache_config: CacheConfig {
            cache_url: std::env::var("CACHE_URL")
                .unwrap_or_else(|_| "https://app.attic.rs/cache".to_string()),
            attic_cache_name: std::env::var("ATTIC_CACHE_NAME")
                .unwrap_or_else(|_| "icicle".to_string()),
        },
    });

    let app = Router::new()
        .route("/api", get(root))
        .route("/health", get(health))
        .merge(webhook::routes())
        .merge(dashboard::routes())
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
