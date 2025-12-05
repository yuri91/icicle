use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub server: ServerConfig,
    pub webhook: WebhookConfig,
    pub cache: CacheConfig,
    pub nix: NixConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Deserialize, Clone)]
pub struct WebhookConfig {
    pub secret: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CacheConfig {
    pub cache_url: String,
    pub attic_cache_name: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct NixConfig {
    /// Timeout for nix-eval-jobs in seconds
    pub eval_timeout_secs: u64,
    /// Default attribute set to evaluate (e.g., "packages.x86_64-linux")
    pub default_attr_set: String,
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        let config_dir = "config";

        let builder = Config::builder()
            // Start with default config file
            .add_source(File::from(Path::new(config_dir).join("default.toml")).required(false))
            // Override with environment-specific config if it exists
            .add_source(File::from(Path::new(config_dir).join("production.toml")).required(false))
            // Override with environment variables
            // Example: ICICLE_SERVER__PORT=8080 or ICICLE_WEBHOOK__SECRET=mysecret
            .add_source(
                Environment::with_prefix("ICICLE")
                    .prefix_separator("_")
                    .separator("__")
                    .try_parsing(true),
            );

        builder.build()?.try_deserialize()
    }

    pub fn with_defaults() -> Self {
        Settings {
            server: ServerConfig {
                host: "0.0.0.0".to_string(),
                port: 3000,
            },
            webhook: WebhookConfig { secret: None },
            cache: CacheConfig {
                cache_url: "https://cache.nixos.org".to_string(),
                attic_cache_name: "icicle".to_string(),
            },
            nix: NixConfig {
                eval_timeout_secs: 300,
                default_attr_set: "packages.x86_64-linux".to_string(),
            },
        }
    }
}
