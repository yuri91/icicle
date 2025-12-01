use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    response::Json,
    routing::post,
    Router,
};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::Value;
use sha2::Sha256;
use std::sync::Arc;
use tracing::{error, info, warn};

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone)]
pub struct WebhookConfig {
    pub secret: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GitHubWebhook {
    #[serde(rename = "ref")]
    pub git_ref: Option<String>,
    pub repository: GitRepository,
    pub after: Option<String>, // commit SHA for push events
    pub head_commit: Option<GitCommit>,
    pub action: Option<String>, // for pull_request events
    pub pull_request: Option<GitPullRequest>,
}

#[derive(Debug, Deserialize)]
pub struct GitRepository {
    pub name: String,
    pub full_name: String,
    pub clone_url: String,
    pub ssh_url: String,
}

#[derive(Debug, Deserialize)]
pub struct GitCommit {
    pub id: String,
    pub message: String,
    pub author: GitAuthor,
}

#[derive(Debug, Deserialize)]
pub struct GitAuthor {
    pub name: String,
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct GitPullRequest {
    pub number: u64,
    pub head: GitPRRef,
}

#[derive(Debug, Deserialize)]
pub struct GitPRRef {
    pub sha: String,
}

pub fn routes() -> Router<Arc<WebhookConfig>> {
    Router::new().route("/webhook/github", post(handle_github_webhook))
}

async fn handle_github_webhook(
    State(config): State<Arc<WebhookConfig>>,
    headers: HeaderMap,
    request: Request,
) -> Result<Json<Value>, StatusCode> {
    // Extract body for signature verification
    let body = axum::body::to_bytes(request.into_body(), usize::MAX)
        .await
        .map_err(|_| {
            error!("Failed to read request body");
            StatusCode::BAD_REQUEST
        })?;

    // Verify GitHub webhook signature if secret is configured
    if let Some(secret) = &config.secret {
        if let Err(status) = verify_signature(&headers, &body, secret) {
            return Err(status);
        }
    } else {
        warn!("Webhook secret not configured - signature verification skipped");
    }

    // Parse the GitHub event
    let event_type = headers
        .get("X-GitHub-Event")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("unknown");

    info!("Received GitHub webhook: {}", event_type);

    // Parse JSON payload
    let webhook: GitHubWebhook = serde_json::from_slice(&body).map_err(|e| {
        error!("Failed to parse webhook JSON: {}", e);
        StatusCode::BAD_REQUEST
    })?;

    // Process the webhook based on event type
    match event_type {
        "push" => handle_push_event(&webhook).await,
        "pull_request" => handle_pull_request_event(&webhook).await,
        _ => {
            info!("Ignoring event type: {}", event_type);
            Ok(Json(serde_json::json!({
                "status": "ignored",
                "message": format!("Event type '{}' is not handled", event_type)
            })))
        }
    }
}

fn verify_signature(headers: &HeaderMap, body: &[u8], secret: &str) -> Result<(), StatusCode> {
    let signature_header = headers
        .get("X-Hub-Signature-256")
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| {
            warn!("Missing X-Hub-Signature-256 header");
            StatusCode::UNAUTHORIZED
        })?;

    if !signature_header.starts_with("sha256=") {
        warn!("Invalid signature format");
        return Err(StatusCode::UNAUTHORIZED);
    }

    let expected_signature = &signature_header[7..]; // Remove "sha256=" prefix

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).map_err(|_| {
        error!("Invalid webhook secret");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    mac.update(body);
    let result = mac.finalize();
    let computed_signature = hex::encode(result.into_bytes());

    if computed_signature != expected_signature {
        warn!("Webhook signature verification failed");
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(())
}

async fn handle_push_event(webhook: &GitHubWebhook) -> Result<Json<Value>, StatusCode> {
    let commit_sha = webhook
        .after
        .as_ref()
        .or_else(|| webhook.head_commit.as_ref().map(|c| &c.id))
        .ok_or_else(|| {
            error!("Push event missing commit SHA");
            StatusCode::BAD_REQUEST
        })?;

    let branch = webhook
        .git_ref
        .as_ref()
        .and_then(|r| r.strip_prefix("refs/heads/"))
        .unwrap_or("unknown");

    info!(
        "Processing push to {} branch {} commit {}",
        webhook.repository.full_name, branch, commit_sha
    );

    // TODO: Create workflow and queue nix evaluation
    // For now, just acknowledge the webhook

    Ok(Json(serde_json::json!({
        "status": "processed",
        "message": "Push event processed",
        "repository": webhook.repository.full_name,
        "branch": branch,
        "commit": commit_sha
    })))
}

async fn handle_pull_request_event(webhook: &GitHubWebhook) -> Result<Json<Value>, StatusCode> {
    let pr = webhook.pull_request.as_ref().ok_or_else(|| {
        error!("Pull request event missing pull_request data");
        StatusCode::BAD_REQUEST
    })?;

    let action = webhook
        .action
        .as_ref()
        .map(|s| s.as_str())
        .unwrap_or("unknown");

    info!(
        "Processing pull request {} action {} for {}, commit {}",
        pr.number, action, webhook.repository.full_name, pr.head.sha
    );

    // Only process certain PR actions
    match action {
        "opened" | "synchronize" | "reopened" => {
            // TODO: Create workflow and queue nix evaluation
            Ok(Json(serde_json::json!({
                "status": "processed",
                "message": "Pull request event processed",
                "repository": webhook.repository.full_name,
                "pr_number": pr.number,
                "action": action,
                "commit": pr.head.sha
            })))
        }
        _ => Ok(Json(serde_json::json!({
            "status": "ignored",
            "message": format!("Pull request action '{}' ignored", action)
        }))),
    }
}
