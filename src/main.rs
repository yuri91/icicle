use axum::{http::StatusCode, response::Json, routing::get, Router};
use serde_json::{json, Value};
use std::{
    net::SocketAddr,
    sync::{atomic::AtomicU64, Arc},
};
use tracing::{info, Level};

mod build;
mod cache;
mod config;
mod dashboard;
mod db;
mod executor;
mod nix;
mod webhook;

use build::BuildQueue;
use cache::CacheConfig;
use config::Settings;
use webhook::WebhookConfig;

pub struct AppState {
    pub build_queue: Arc<BuildQueue>,
    pub workflow_counter: AtomicU64,
    pub webhook_config: WebhookConfig,
    pub cache_config: CacheConfig,
    pub db_pool: sqlx::SqlitePool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    // Load configuration
    let settings = Settings::new().unwrap_or_else(|e| {
        tracing::warn!("Failed to load configuration: {}. Using defaults.", e);
        Settings::with_defaults()
    });

    info!("Configuration loaded:");
    info!(
        "  Server: {}:{}",
        settings.server.host, settings.server.port
    );
    info!("  Cache URL: {}", settings.cache.cache_url);
    info!("  Attic cache: {}", settings.cache.attic_cache_name);
    info!("  Nix eval timeout: {}s", settings.nix.eval_timeout_secs);
    info!(
        "  Webhook secret configured: {}",
        settings.webhook.secret.is_some()
    );

    // Initialize database
    info!("Initializing database at: {}", settings.database.path);
    let db_pool = db::init_database(&settings.database.path).await?;
    info!("Database initialized successfully");

    // Initialize app state
    let build_queue = Arc::new(BuildQueue::new());

    let app_state = Arc::new(AppState {
        build_queue: build_queue.clone(),
        workflow_counter: AtomicU64::new(0),
        webhook_config: WebhookConfig {
            secret: settings.webhook.secret.clone(),
            attrset: settings.nix.default_attr_set.clone(),
        },
        cache_config: CacheConfig {
            cache_url: settings.cache.cache_url.clone(),
            attic_cache_name: settings.cache.attic_cache_name.clone(),
        },
        db_pool: db_pool.clone(),
    });

    // Initialize and spawn build executor
    let executor = Arc::new(executor::BuildExecutor::new(
        build_queue,
        db_pool,
        cache::CacheClient::new(app_state.cache_config.clone()),
        settings.build.max_concurrent_builds,
        settings.build.build_timeout_secs,
    ));

    tokio::spawn(async move {
        executor.run().await;
    });

    let app = Router::new()
        .route("/api", get(root))
        .route("/health", get(health))
        .merge(webhook::routes())
        .merge(dashboard::routes())
        .with_state(app_state);

    let addr = SocketAddr::from((
        settings
            .server
            .host
            .parse::<std::net::IpAddr>()
            .unwrap_or_else(|_| std::net::IpAddr::V4(std::net::Ipv4Addr::new(0, 0, 0, 0))),
        settings.server.port,
    ));
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
