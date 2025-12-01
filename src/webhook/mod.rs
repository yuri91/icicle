use axum::{http::StatusCode, response::Json, routing::post, Router};
use serde_json::Value;

pub fn routes() -> Router {
    Router::new().route("/webhook/github", post(handle_github_webhook))
}

async fn handle_github_webhook() -> Result<Json<Value>, StatusCode> {
    Ok(Json(serde_json::json!({
        "status": "received",
        "message": "GitHub webhook processed"
    })))
}
