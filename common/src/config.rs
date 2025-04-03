// common/src/config.rs
use serde::{Deserialize, Serialize};
use std::env;
use std::path::PathBuf;
use config::{Config as ConfigFile, File, Environment};

/// Central configuration for both services
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub websocket_server_addr: String,
    pub web_server_addr: String,
    pub agent_token: String,  // Pre-shared key for agent authentication
    
    // Static file serving configuration
    pub static_files: StaticFilesConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StaticFilesConfig {
    pub path: String,
    pub index: String,
    pub enable_compression: bool,
    pub cache: CacheConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CacheConfig {
    pub max_age: u32,
    pub immutable: bool,
    pub must_revalidate: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            websocket_server_addr: "127.0.0.1:8080".to_string(),
            web_server_addr: "127.0.0.1:8081".to_string(),
            agent_token: "dev_token".to_string(),
            
            static_files: StaticFilesConfig {
                path: "./static".to_string(),
                index: "index.html".to_string(),
                enable_compression: true,
                cache: CacheConfig {
                    max_age: 3600,
                    immutable: false,
                    must_revalidate: true,
                },
            },
        }
    }
}

impl Config {
    /// Load configuration from file and environment
    pub fn load() -> Result<Self, config::ConfigError> {
        // Get the run mode, defaulting to "development"
        let run_mode = env::var("RUN_MODE").unwrap_or_else(|_| "development".into());
        
        // Locate the config directory
        let config_dir = env::var("CONFIG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                // Check if we're in the project root or a subcrate
                let mut path = PathBuf::from("./config");
                if !path.exists() {
                    path = PathBuf::from("../config");
                }
                path
            });
            
        tracing::info!("Loading configuration from {}", config_dir.display());
        tracing::info!("Using run mode: {}", run_mode);
        
        // Build configuration
        let config = ConfigFile::builder()
            // Start with defaults
            .add_source(File::from(config_dir.join("default.toml")).required(false))
            // Add environment specific config
            .add_source(File::from(config_dir.join(format!("{}.toml", run_mode))).required(false))
            // Add a local config file for local overrides
            .add_source(File::from(config_dir.join("local.toml")).required(false))
            // Add environment variables with prefix "APP"
            .add_source(Environment::with_prefix("APP").separator("__"))
            // Build and deserialize
            .build()?
            .try_deserialize()?;
            
        Ok(config)
    }
    
    /// Load from environment variables directly (backward compatibility)
    pub fn from_env() -> Self {
        // Try to load from file first
        match Self::load() {
            Ok(config) => {
                tracing::info!("Configuration loaded from files and environment");
                config
            },
            Err(e) => {
                tracing::warn!("Failed to load configuration from files: {}", e);
                tracing::info!("Falling back to environment variables only");
                
                // Fall back to the old method
                let websocket_server_addr = env::var("WEBSOCKET_SERVER_ADDR")
                    .unwrap_or_else(|_| "127.0.0.1:8080".to_string());
                    
                let web_server_addr = env::var("WEB_SERVER_ADDR")
                    .unwrap_or_else(|_| "127.0.0.1:8081".to_string());
                    
                let agent_token = env::var("AGENT_TOKEN")
                    .unwrap_or_else(|_| "dev_token".to_string());
                
                // Static file serving configuration
                let static_files_path = env::var("STATIC_FILES_PATH")
                    .unwrap_or_else(|_| "./static".to_string());
                    
                let static_files_index = env::var("STATIC_FILES_INDEX")
                    .unwrap_or_else(|_| "index.html".to_string());
                    
                let enable_compression = env::var("ENABLE_COMPRESSION")
                    .map(|v| v.to_lowercase() == "true")
                    .unwrap_or(true);
                    
                let cache_max_age = env::var("CACHE_MAX_AGE")
                    .ok()
                    .and_then(|v| v.parse::<u32>().ok())
                    .unwrap_or(3600);
                    
                let cache_immutable = env::var("CACHE_IMMUTABLE")
                    .map(|v| v.to_lowercase() == "true")
                    .unwrap_or(false);
                    
                let cache_must_revalidate = env::var("CACHE_MUST_REVALIDATE")
                    .map(|v| v.to_lowercase() == "true")
                    .unwrap_or(true);
                
                Self {
                    websocket_server_addr,
                    web_server_addr,
                    agent_token,
                    static_files: StaticFilesConfig {
                        path: static_files_path,
                        index: static_files_index,
                        enable_compression,
                        cache: CacheConfig {
                            max_age: cache_max_age,
                            immutable: cache_immutable,
                            must_revalidate: cache_must_revalidate,
                        },
                    },
                }
            }
        }
    }
}