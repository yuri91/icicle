use anyhow::{anyhow, Context, Result};
use std::process::Stdio;
use tokio::process::Command;
use tracing::{info, warn};

#[derive(Debug, Clone)]
pub struct CacheConfig {
    pub cache_url: String,
    pub attic_cache_name: String,
}

pub struct CacheClient {
    config: CacheConfig,
}

impl CacheClient {
    pub fn new(config: CacheConfig) -> Self {
        Self { config }
    }

    /// Check if a store path exists in the cache using nix path-info
    pub async fn path_exists(&self, store_path: &str) -> Result<bool> {
        info!("Checking cache for store path: {}", store_path);

        let output = Command::new("nix")
            .args(["path-info", "--store", &self.config.cache_url, store_path])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .context("Failed to execute nix path-info")?;

        match output.status.success() {
            true => {
                info!("Cache HIT: {} found in cache", store_path);
                Ok(true)
            }
            false => {
                info!("Cache MISS: {} not found in cache", store_path);
                Ok(false)
            }
        }
    }

    /// Check if any of the store paths exist in the cache (short-circuits on first hit)
    pub async fn any_path_exists(&self, store_paths: &[String]) -> Result<bool> {
        for path in store_paths {
            match self.path_exists(path).await {
                Ok(true) => return Ok(true), // Short-circuit on first hit
                Ok(false) => continue,
                Err(e) => {
                    warn!("Failed to check cache for {}: {}", path, e);
                    // Continue checking other paths on error
                }
            }
        }
        Ok(false)
    }

    /// Check if all outputs of a derivation are cached
    pub async fn derivation_cached(&self, outputs: &[String]) -> Result<bool> {
        if outputs.is_empty() {
            return Ok(true); // No outputs to check
        }

        let mut cached_count = 0;
        for output in outputs {
            match self.path_exists(output).await {
                Ok(true) => cached_count += 1,
                Ok(false) => {
                    info!("Output {} not cached", output);
                    return Ok(false); // Short-circuit on first miss
                }
                Err(e) => {
                    warn!("Failed to check cache for {}: {}", output, e);
                    return Ok(false); // Assume not cached on error
                }
            }
        }

        info!("All {} outputs are cached", cached_count);
        Ok(true)
    }

    /// Upload a store path to the cache using attic
    pub async fn upload_path(&self, store_path: &str) -> Result<()> {
        info!("Uploading {} to cache", store_path);

        let output = Command::new("attic")
            .args(["push", &self.config.attic_cache_name, store_path])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .context("Failed to execute attic push")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Failed to upload to cache: {}", stderr));
        }

        info!("Successfully uploaded {} to cache", store_path);
        Ok(())
    }

    /// Upload all outputs of a derivation to the cache
    pub async fn upload_derivation_outputs(&self, outputs: &[String]) -> Result<()> {
        for output in outputs {
            if let Err(e) = self.upload_path(output).await {
                warn!("Failed to upload {}: {}", output, e);
                // Continue uploading other outputs even if one fails
            }
        }
        Ok(())
    }
}
