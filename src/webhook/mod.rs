use crate::{
    build::{BuildStatus, Workflow, WorkflowStatus},
    cache::CacheClient,
    nix::NixEvaluator,
};
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

pub fn routes() -> Router<Arc<crate::AppState>> {
    Router::new().route("/webhook/github", post(handle_github_webhook))
}

async fn handle_github_webhook(
    State(app_state): State<Arc<crate::AppState>>,
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
    if let Some(secret) = &app_state.webhook_config.secret {
        verify_signature(&headers, &body, secret)?
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
        "push" => handle_push_event(&app_state, &webhook).await,
        "pull_request" => handle_pull_request_event(&app_state, &webhook).await,
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

async fn handle_push_event(
    app_state: &Arc<crate::AppState>,
    webhook: &GitHubWebhook,
) -> Result<Json<Value>, StatusCode> {
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

    // Create workflow and trigger nix evaluation
    let workflow_id = create_workflow(
        app_state,
        &webhook.repository.full_name,
        commit_sha,
        branch,
        &webhook.repository.clone_url,
    )
    .await
    .map_err(|e| {
        error!("Failed to create workflow: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(serde_json::json!({
        "status": "processed",
        "message": "Push event processed",
        "repository": webhook.repository.full_name,
        "branch": branch,
        "commit": commit_sha,
        "workflow_id": workflow_id
    })))
}

async fn handle_pull_request_event(
    app_state: &Arc<crate::AppState>,
    webhook: &GitHubWebhook,
) -> Result<Json<Value>, StatusCode> {
    let pr = webhook.pull_request.as_ref().ok_or_else(|| {
        error!("Pull request event missing pull_request data");
        StatusCode::BAD_REQUEST
    })?;

    let action = webhook.action.as_deref().unwrap_or("unknown");

    info!(
        "Processing pull request {} action {} for {}, commit {}",
        pr.number, action, webhook.repository.full_name, pr.head.sha
    );

    // Only process certain PR actions
    match action {
        "opened" | "synchronize" | "reopened" => {
            let workflow_id = create_workflow(
                app_state,
                &webhook.repository.full_name,
                &pr.head.sha,
                &format!("pr-{}", pr.number),
                &webhook.repository.clone_url,
            )
            .await
            .map_err(|e| {
                error!("Failed to create workflow: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

            Ok(Json(serde_json::json!({
                "status": "processed",
                "message": "Pull request event processed",
                "repository": webhook.repository.full_name,
                "pr_number": pr.number,
                "action": action,
                "commit": pr.head.sha,
                "workflow_id": workflow_id
            })))
        }
        _ => Ok(Json(serde_json::json!({
            "status": "ignored",
            "message": format!("Pull request action '{}' ignored", action)
        }))),
    }
}

async fn create_workflow(
    app_state: &Arc<crate::AppState>,
    repository: &str,
    commit_sha: &str,
    branch: &str,
    clone_url: &str,
) -> Result<i64, anyhow::Error> {
    // TODO: Make attribute set configurable per repository
    let attribute_set = "packages.x86_64-linux";
    let now = chrono::Utc::now().timestamp();

    // Insert workflow into database to get auto-generated ID
    let workflow_id = sqlx::query!(
        r#"
        INSERT INTO workflows (repository, commit_sha, attribute_set, status, created_at)
        VALUES (?, ?, ?, 'Pending', ?)
        "#,
        repository,
        commit_sha,
        attribute_set,
        now
    )
    .execute(&app_state.db_pool)
    .await?
    .last_insert_rowid();

    info!(
        "Creating workflow {} for {} at {} ({})",
        workflow_id, repository, commit_sha, branch
    );

    // Spawn background task to process the workflow
    let app_state_clone = app_state.clone();
    let repository = repository.to_string();
    let commit_sha = commit_sha.to_string();
    let clone_url = clone_url.to_string();

    tokio::spawn(async move {
        if let Err(e) = process_workflow(
            &app_state_clone,
            workflow_id,
            &repository,
            &commit_sha,
            &clone_url,
        )
        .await
        {
            error!("Failed to process workflow {}: {}", workflow_id, e);
        }
    });

    Ok(workflow_id)
}

async fn process_workflow(
    app_state: &Arc<crate::AppState>,
    workflow_id: i64,
    repository: &str,
    commit_sha: &str,
    clone_url: &str,
) -> Result<(), anyhow::Error> {
    info!("Processing workflow {} for {}", workflow_id, repository);

    // TODO: Make attribute set configurable per repository
    let attribute_set = "packages.x86_64-linux";

    // Update workflow status to Running
    sqlx::query!(
        r#"
        UPDATE workflows SET status = 'Running' WHERE id = ?
        "#,
        workflow_id
    )
    .execute(&app_state.db_pool)
    .await?;

    // Create workflow object
    let _workflow = Workflow {
        id: workflow_id,
        repository: repository.to_string(),
        commit_sha: commit_sha.to_string(),
        attribute_set: attribute_set.to_string(),
        status: WorkflowStatus::Running,
    };

    // Evaluate the repository
    let mut evaluator = NixEvaluator::new();
    let derivations = evaluator
        .evaluate_repository(clone_url, commit_sha, attribute_set)
        .await?;

    info!(
        "Found {} derivations for workflow {}",
        derivations.len(),
        workflow_id
    );

    // Check cache and add jobs to the build queue
    let cache_client = CacheClient::new(app_state.cache_config.clone());
    let mut jobs_queued = 0;
    let mut jobs_cached = 0;

    {
        let mut queue = app_state.build_queue.lock().await;
        for mut derivation in derivations {
            // Check if derivation outputs are already cached
            match cache_client.derivation_cached(&derivation.outputs).await {
                Ok(true) => {
                    info!("Skipping cached derivation: {}", derivation.name);
                    jobs_cached += 1;
                    // Still add to queue but mark as cached
                    derivation.status = BuildStatus::Cached;
                    queue.add_job(derivation, workflow_id);
                }
                Ok(false) => {
                    info!("Queueing derivation for build: {}", derivation.name);
                    jobs_queued += 1;
                    queue.add_job(derivation, workflow_id);
                }
                Err(e) => {
                    warn!("Failed to check cache for {}: {}", derivation.name, e);
                    // Queue the job anyway if cache check fails
                    info!(
                        "Queueing derivation due to cache check failure: {}",
                        derivation.name
                    );
                    jobs_queued += 1;
                    queue.add_job(derivation, workflow_id);
                }
            }
        }
    }

    info!(
        "Workflow {} processed: {} jobs queued, {} jobs cached",
        workflow_id, jobs_queued, jobs_cached
    );
    Ok(())
}
