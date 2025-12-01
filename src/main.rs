use axum::{http::StatusCode, response::Json, routing::get, Router};
use serde_json::{json, Value};
use std::net::SocketAddr;
use tracing::{info, Level};
use tracing_subscriber;

mod build;
mod webhook;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    let app = Router::new()
        .route("/", get(root))
        .route("/health", get(health))
        .merge(webhook::routes());

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
