use crate::build::Derivation;
use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use tempfile::TempDir;
use tokio::process::Command;
use tracing::{error, info, warn};

#[derive(Debug, Deserialize)]
pub struct NixEvalJob {
    pub attr: String,
    #[serde(rename = "drvPath")]
    pub drv_path: String,
    pub outputs: HashMap<String, NixOutput>,
    pub system: String,
}

#[derive(Debug, Deserialize)]
pub struct NixOutput {
    pub path: String,
}

pub struct NixEvaluator {
    temp_dir: Option<TempDir>,
}

impl NixEvaluator {
    pub fn new() -> Self {
        Self { temp_dir: None }
    }

    /// Clone a git repository to a temporary directory
    pub async fn clone_repository(&mut self, clone_url: &str, commit_sha: &str) -> Result<()> {
        info!("Cloning repository {} at commit {}", clone_url, commit_sha);

        let temp_dir = tempfile::tempdir()
            .context("Failed to create temporary directory for repository clone")?;

        let repo_path = temp_dir.path();

        // Clone the repository
        let clone_output = Command::new("git")
            .args([
                "clone",
                "--depth=1",
                "--no-single-branch",
                clone_url,
                repo_path.to_str().unwrap(),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .context("Failed to execute git clone")?;

        if !clone_output.status.success() {
            let stderr = String::from_utf8_lossy(&clone_output.stderr);
            return Err(anyhow!("Git clone failed: {}", stderr));
        }

        // Checkout the specific commit
        let checkout_output = Command::new("git")
            .current_dir(repo_path)
            .args(["checkout", commit_sha])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .context("Failed to execute git checkout")?;

        if !checkout_output.status.success() {
            let stderr = String::from_utf8_lossy(&checkout_output.stderr);
            return Err(anyhow!("Git checkout failed: {}", stderr));
        }

        info!("Successfully cloned repository to {:?}", repo_path);
        self.temp_dir = Some(temp_dir);
        Ok(())
    }

    /// Get the path to the cloned repository
    pub fn repo_path(&self) -> Option<&Path> {
        self.temp_dir.as_ref().map(|td| td.path())
    }

    /// Evaluate a flake attribute set using nix-eval-jobs
    pub async fn evaluate_flake(
        &self,
        repo_path: &Path,
        attribute_set: &str,
    ) -> Result<Vec<Derivation>> {
        info!(
            "Evaluating flake at {:?} for attribute set: {}",
            repo_path, attribute_set
        );

        let flake_path = repo_path.join("flake.nix");
        if !flake_path.exists() {
            return Err(anyhow!(
                "No flake.nix found in repository at {:?}",
                repo_path
            ));
        }

        // Run nix-eval-jobs to get the discrete jobs
        let output = Command::new("nix-eval-jobs")
            .current_dir(repo_path)
            .args([
                "--flake",
                &format!(".#{}", attribute_set),
                "--json",
                "--show-trace",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .context("Failed to execute nix-eval-jobs")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("nix-eval-jobs failed: {}", stderr);
            return Err(anyhow!("nix-eval-jobs evaluation failed: {}", stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        info!("nix-eval-jobs completed successfully");

        // Parse the jobs
        let mut jobs = Vec::new();
        for line in stdout.lines() {
            if line.trim().is_empty() {
                continue;
            }

            match serde_json::from_str::<NixEvalJob>(line) {
                Ok(job) => {
                    jobs.push(job);
                }
                Err(e) => {
                    warn!("Failed to parse nix-eval-jobs output line: {}", e);
                    warn!("Line content: {}", line);
                }
            }
        }

        info!("Found {} discrete jobs from evaluation", jobs.len());

        // Now find transitive dependencies between jobs
        let derivations = self.resolve_job_dependencies(repo_path, jobs).await?;

        info!(
            "Successfully resolved dependencies for {} derivations",
            derivations.len()
        );
        Ok(derivations)
    }

    /// Find transitive dependencies between jobs using nix-store --query
    async fn resolve_job_dependencies(
        &self,
        repo_path: &Path,
        jobs: Vec<NixEvalJob>,
    ) -> Result<Vec<Derivation>> {
        let mut derivations = Vec::new();
        let mut drv_to_job: HashMap<String, usize> = HashMap::new();

        // Create derivation objects and build lookup map
        for (i, job) in jobs.iter().enumerate() {
            let outputs: Vec<String> = job.outputs.values().map(|o| o.path.clone()).collect();

            let derivation = Derivation {
                name: job.attr.clone(),
                drv_path: job.drv_path.clone(),
                outputs,
                system: job.system.clone(),
                input_drvs: Vec::new(), // Will be filled in later
            };

            derivations.push(derivation);
            drv_to_job.insert(job.drv_path.clone(), i);
        }

        // For each job, find its transitive dependencies and filter to only include other jobs
        for (i, job) in jobs.iter().enumerate() {
            let job_dependencies = self
                .get_transitive_dependencies(repo_path, &job.drv_path)
                .await?;

            // Filter to only include dependencies that are also jobs (not intermediate derivations)
            let input_job_drvs: Vec<String> = job_dependencies
                .into_iter()
                .filter(|drv_path| drv_to_job.contains_key(drv_path))
                .collect();

            derivations[i].input_drvs = input_job_drvs;
        }

        Ok(derivations)
    }

    /// Get transitive dependencies of a derivation using nix-store --query --requisites
    async fn get_transitive_dependencies(
        &self,
        repo_path: &Path,
        drv_path: &str,
    ) -> Result<Vec<String>> {
        let output = Command::new("nix-store")
            .current_dir(repo_path)
            .args(["--query", "--requisites", drv_path])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .context("Failed to execute nix-store --query")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("nix-store query failed for {}: {}", drv_path, stderr);
            return Ok(Vec::new()); // Return empty deps rather than failing
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let dependencies: Vec<String> = stdout
            .lines()
            .filter(|line| line.ends_with(".drv"))
            .map(|line| line.to_string())
            .collect();

        Ok(dependencies)
    }

    /// Evaluate a repository for a specific attribute set
    pub async fn evaluate_repository(
        &mut self,
        clone_url: &str,
        commit_sha: &str,
        attribute_set: &str,
    ) -> Result<Vec<Derivation>> {
        self.clone_repository(clone_url, commit_sha).await?;
        let repo_path = self.repo_path().unwrap();
        self.evaluate_flake(repo_path, attribute_set).await
    }
}

impl Drop for NixEvaluator {
    fn drop(&mut self) {
        if let Some(temp_dir) = &self.temp_dir {
            info!("Cleaning up temporary directory: {:?}", temp_dir.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_nix_eval_job_parsing() {
        let json = r#"{"attr":"packages.x86_64-linux.hello","drvPath":"/nix/store/abc123-hello.drv","outputs":{"out":{"path":"/nix/store/def456-hello"}},"system":"x86_64-linux"}"#;

        let job: NixEvalJob = serde_json::from_str(json).unwrap();
        assert_eq!(job.attr, "packages.x86_64-linux.hello");
        assert_eq!(job.system, "x86_64-linux");
        assert!(job.outputs.contains_key("out"));
    }
}
