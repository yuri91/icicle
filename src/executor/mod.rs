use crate::{
    build::{BuildJob, BuildQueue, BuildStatus},
    cache::CacheClient,
};
use sqlx::SqlitePool;
use std::{collections::VecDeque, sync::Arc};
use tokio::sync::Semaphore;
use tokio::time::{timeout, Duration};
use tracing::{error, info, warn};

pub struct BuildExecutor {
    build_queue: Arc<BuildQueue>,
    db_pool: SqlitePool,
    cache_client: CacheClient,
    max_concurrent_builds: usize,
    build_timeout: Duration,
}

impl BuildExecutor {
    pub fn new(
        build_queue: Arc<BuildQueue>,
        db_pool: SqlitePool,
        cache_client: CacheClient,
        max_concurrent_builds: usize,
        build_timeout_secs: u64,
    ) -> Self {
        Self {
            build_queue,
            db_pool,
            cache_client,
            max_concurrent_builds,
            build_timeout: Duration::from_secs(build_timeout_secs),
        }
    }

    /// Start the build executor loop
    pub async fn run(self: Arc<Self>) {
        info!(
            "Build executor started with max {} concurrent builds",
            self.max_concurrent_builds
        );

        let mut run_queue = VecDeque::new();
        let semaphore = Arc::new(Semaphore::new(self.max_concurrent_builds));
        loop {
            if run_queue.is_empty() {
                self.build_queue.wait_for_ready_jobs().await;
                run_queue.extend(self.build_queue.drain_ready_jobs());
            }
            info!("Got {} jobs to run", run_queue.len());
            assert!(!run_queue.is_empty());
            let job = run_queue.pop_front().unwrap();
            if job.status.error() {
                continue;
            }
            assert!(job.status == BuildStatus::Ready);
            let permit = semaphore.clone().acquire_owned().await.unwrap();
            let executor = self.clone();
            tokio::spawn(async move {
                let res = executor.execute_build(job.clone()).await;
                drop(permit);
                if let Err(e) = res {
                    error!(
                        "Build execution error for {}: {}",
                        job.derivation.drv_path, e
                    );
                }
            });
        }
    }

    /// Execute a single build
    async fn execute_build(&self, job: BuildJob) -> anyhow::Result<()> {
        let drv_path = job.derivation.drv_path;
        info!("Checking cache status for derivation: {}", drv_path);
        let status = if self
            .cache_client
            .derivation_cached(&job.derivation.outputs)
            .await?
        {
            info!("Derivation {} is cached", drv_path);
            BuildStatus::Cached
        } else {
            info!("Derivation {} is NOT cached", drv_path);
            BuildStatus::Running
        };

        self.build_queue.update_status(&drv_path, status);
        // Update database
        let now = chrono::Utc::now().timestamp();
        if let Err(e) = sqlx::query(
            r#"
                INSERT INTO builds (drv_path, name, system, status, started_at)
                VALUES (?, ?, ?, 'Running', ?)
                ON CONFLICT(drv_path) DO UPDATE SET status = 'Running', started_at = ?
                "#,
        )
        .bind(&drv_path)
        .bind(&job.derivation.name)
        .bind(&job.derivation.system)
        .bind(now)
        .bind(now)
        .execute(&self.db_pool)
        .await
        {
            warn!("Failed to update build status in database: {}", e);
        }
        for workflow_id in &job.requested_by {
            // Link build to workflow
            if let Err(e) = sqlx::query(
                r#"
                INSERT OR IGNORE INTO build_workflows (drv_path, workflow_id)
                VALUES (?, ?)
                "#,
            )
            .bind(&drv_path)
            .bind(workflow_id)
            .execute(&self.db_pool)
            .await
            {
                warn!("Failed to link build to workflow: {}", e);
            }
        }
        if status == BuildStatus::Cached {
            return Ok(());
        }

        info!("Starting build for derivation: {}", drv_path);
        // Execute the build with timeout
        let result = timeout(self.build_timeout, self.run_nix_build(&drv_path)).await;

        // Process result and update status
        let (final_status, error_message) = match result {
            Ok(Ok(())) => {
                info!("Build succeeded: {}", drv_path);

                // Upload to cache
                if let Err(e) = self.upload_to_cache(&drv_path).await {
                    warn!("Failed to upload {} to cache: {}", drv_path, e);
                }

                (BuildStatus::Success, None)
            }
            Ok(Err(e)) => {
                error!("Build failed for {}: {}", drv_path, e);
                (BuildStatus::Failed, Some(e.to_string()))
            }
            Err(_) => {
                error!("Build timed out for {}", drv_path);
                (
                    BuildStatus::Failed,
                    Some(format!(
                        "Build timed out after {} seconds",
                        self.build_timeout.as_secs()
                    )),
                )
            }
        };

        // Update queue status
        self.build_queue.update_status(&drv_path, final_status);

        // Update database
        let finished_at = chrono::Utc::now().timestamp();
        if let Err(e) = sqlx::query(
            r#"
            UPDATE builds
            SET status = ?, finished_at = ?, error_message = ?
            WHERE drv_path = ?
            "#,
        )
        .bind(final_status.to_string())
        .bind(finished_at)
        .bind(error_message)
        .bind(drv_path)
        .execute(&self.db_pool)
        .await
        {
            warn!("Failed to update final build status in database: {}", e);
        }

        Ok(())
    }

    /// Run nix-build for a derivation
    async fn run_nix_build(&self, drv_path: &str) -> anyhow::Result<()> {
        info!("Executing: nix-build {}", drv_path);

        let output = tokio::process::Command::new("nix-build")
            .arg(drv_path)
            .output()
            .await?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow::anyhow!("nix-build failed: {}", stderr))
        }
    }

    /// Upload build outputs to cache
    async fn upload_to_cache(&self, drv_path: &str) -> anyhow::Result<()> {
        info!("Uploading {} to cache", drv_path);

        // Query the outputs of the derivation
        let output = tokio::process::Command::new("nix-store")
            .args(&["--query", "--outputs", drv_path])
            .output()
            .await?;

        if !output.status.success() {
            return Err(anyhow::anyhow!("Failed to query derivation outputs"));
        }

        let outputs_str = String::from_utf8(output.stdout)?;
        let outputs: Vec<String> = outputs_str
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        // Upload each output
        self.cache_client
            .upload_derivation_outputs(&outputs)
            .await?;

        Ok(())
    }
}
