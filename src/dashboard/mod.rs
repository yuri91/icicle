use crate::build::{BuildJob, BuildStatus};
use askama::Template;
use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use std::{collections::HashMap, sync::Arc};

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate {
    job_queue: JobQueueSection,
    workflows: WorkflowSection,
}

struct JobQueueSection {
    jobs: Vec<JobInfo>,
    stats: QueueStats,
}

struct WorkflowSection {
    workflows: Vec<WorkflowInfo>,
}

#[derive(Clone)]
struct JobInfo {
    name: String,
    drv_path: String,
    system: String,
    status: BuildStatus,
    requested_by_count: usize,
    outputs: Vec<String>,
}

struct QueueStats {
    total: usize,
    queued: usize,
    ready: usize,
    running: usize,
    success: usize,
    failed: usize,
    cached: usize,
    timedout: usize,
    canceled: usize,
}

#[derive(Clone)]
struct WorkflowInfo {
    id: i64,
    job_details: Vec<WorkflowJobDetail>,
    summary: WorkflowSummary,
}

#[derive(Clone)]
struct WorkflowJobDetail {
    name: String,
    status: BuildStatus,
}

#[derive(Clone)]
struct WorkflowSummary {
    total_jobs: usize,
    completed_jobs: usize,
    failed_jobs: usize,
    cached_jobs: usize,
    progress_percent: u8,
}

pub fn routes() -> Router<Arc<crate::AppState>> {
    Router::new()
        .route("/", get(dashboard))
        .route("/dashboard", get(dashboard))
}

async fn dashboard(
    State(app_state): State<Arc<crate::AppState>>,
) -> Result<impl IntoResponse, StatusCode> {
    // Build Job Queue Section
    let job_queue = build_job_queue_section(&app_state.build_queue);

    // Build Workflows Section
    let workflows = build_workflow_section(&app_state.build_queue);

    let template = DashboardTemplate {
        job_queue,
        workflows,
    };

    match template.render() {
        Ok(html) => Ok(Html(html)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

fn build_job_queue_section(queue: &crate::build::BuildQueue) -> JobQueueSection {
    let mut jobs = Vec::new();
    let mut stats = QueueStats {
        total: 0,
        queued: 0,
        ready: 0,
        running: 0,
        success: 0,
        failed: 0,
        cached: 0,
        timedout: 0,
        canceled: 0,
    };

    for job in queue.get_jobs() {
        stats.total += 1;

        match job.status {
            BuildStatus::Queued => stats.queued += 1,
            BuildStatus::Ready => stats.ready += 1,
            BuildStatus::Running => stats.running += 1,
            BuildStatus::Success => stats.success += 1,
            BuildStatus::Failed => stats.failed += 1,
            BuildStatus::Cached => stats.cached += 1,
            BuildStatus::Timedout => stats.timedout += 1,
            BuildStatus::Canceled => stats.canceled += 1,
        }

        jobs.push(JobInfo {
            name: job.derivation.name.clone(),
            drv_path: job.derivation.drv_path.clone(),
            system: job.derivation.system.clone(),
            status: job.status.clone(),
            requested_by_count: job.requested_by.len(),
            outputs: job.derivation.outputs.clone(),
        });
    }

    // Sort jobs by priority (running first, then queued, etc.)
    jobs.sort_by(|a, b| {
        let priority = |status: &BuildStatus| match status {
            BuildStatus::Running => 0,
            BuildStatus::Ready => 1,
            BuildStatus::Queued => 2,
            BuildStatus::Failed => 3,
            BuildStatus::Timedout => 4,
            BuildStatus::Canceled => 5,
            BuildStatus::Success => 6,
            BuildStatus::Cached => 7,
        };
        priority(&a.status).cmp(&priority(&b.status))
    });

    JobQueueSection { jobs, stats }
}

fn build_workflow_section(queue: &crate::build::BuildQueue) -> WorkflowSection {
    let mut workflow_map: HashMap<i64, Vec<BuildJob>> = HashMap::new();

    // Group jobs by workflow
    for job in queue.get_jobs() {
        for workflow_id in &job.requested_by {
            workflow_map
                .entry(*workflow_id)
                .or_default()
                .push(job.clone());
        }
    }

    let mut workflows = Vec::new();
    for (workflow_id, jobs) in workflow_map {
        // Build job details for this workflow
        let mut job_details = Vec::new();
        for job in &jobs {
            job_details.push(WorkflowJobDetail {
                name: job.derivation.name.clone(),
                status: job.derivation.status.clone(),
            });
        }

        // Calculate workflow summary
        let total_jobs = jobs.len();
        let completed_jobs = jobs
            .iter()
            .filter(|j| matches!(j.derivation.status, BuildStatus::Success))
            .count();
        let failed_jobs = jobs
            .iter()
            .filter(|j| matches!(j.derivation.status, BuildStatus::Failed))
            .count();
        let cached_jobs = jobs
            .iter()
            .filter(|j| matches!(j.derivation.status, BuildStatus::Cached))
            .count();

        let finished_jobs = completed_jobs + failed_jobs + cached_jobs;
        let progress_percent = if total_jobs > 0 {
            ((finished_jobs * 100) / total_jobs) as u8
        } else {
            0
        };

        workflows.push(WorkflowInfo {
            id: workflow_id,
            job_details,
            summary: WorkflowSummary {
                total_jobs,
                completed_jobs,
                failed_jobs,
                cached_jobs,
                progress_percent,
            },
        });
    }

    // Sort workflows by ID (most recent first)
    workflows.sort_by(|a, b| b.id.cmp(&a.id));

    WorkflowSection { workflows }
}
