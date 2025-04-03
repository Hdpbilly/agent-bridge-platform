// web-server/src/static_files.rs
use actix_web::{web, HttpRequest, Result, Error};
use actix_web::middleware::Compress;
use actix_web::http::header;
use actix_files::{Files, NamedFile};
use std::path::PathBuf;

// Configuration for static file serving
#[derive(Clone)]
pub struct StaticFilesConfig {
    pub root_path: PathBuf, 
    pub index_file: String,
    pub enable_compression: bool,
    pub cache_control: CacheControl,
}

// Caching configuration
#[derive(Clone, Debug)]
pub struct CacheControl {
    pub max_age: u32,           // Max age in seconds
    pub immutable: bool,        // Whether to add immutable directive
    pub must_revalidate: bool,  // Whether client must revalidate after max_age
}

impl Default for CacheControl {
    fn default() -> Self {
        Self {
            max_age: 3600,  // 1 hour
            immutable: false,
            must_revalidate: true,
        }
    }
}

impl Default for StaticFilesConfig {
    fn default() -> Self {
        Self {
            root_path: PathBuf::from("./static"),
            index_file: "index.html".to_string(),
            enable_compression: true,
            cache_control: CacheControl::default(),
        }
    }
}

impl StaticFilesConfig {
    pub fn from_env() -> Self {
        let root_path = std::env::var("STATIC_ASSETS_PATH")
            .unwrap_or_else(|_| "./static".to_string());
        
        // Get compression settings from environment variables
        let enable_compression = std::env::var("ENABLE_COMPRESSION")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(true);
        
        // Get cache settings from environment variables
        let cache_max_age = std::env::var("CACHE_MAX_AGE")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(3600);
            
        let cache_immutable = std::env::var("CACHE_IMMUTABLE")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(false);
            
        let cache_must_revalidate = std::env::var("CACHE_MUST_REVALIDATE")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(true);
        
        Self {
            root_path: PathBuf::from(root_path),
            index_file: "index.html".to_string(),
            enable_compression,
            cache_control: CacheControl {
                max_age: cache_max_age,
                immutable: cache_immutable,
                must_revalidate: cache_must_revalidate,
            },
        }
    }
}

// Function to build cache control header value
fn build_cache_control_value(config: &CacheControl) -> String {
    let mut directives = vec![format!("max-age={}", config.max_age)];
    
    if config.immutable {
        directives.push("immutable".to_string());
    }
    
    if config.must_revalidate {
        directives.push("must-revalidate".to_string());
    }
    
    directives.join(", ")
}

// SPA fallback handler for client-side routing
async fn spa_index(req: HttpRequest, config: web::Data<StaticFilesConfig>) -> Result<NamedFile, Error> {
    // Don't serve index.html for API or WebSocket routes
    let path = req.path();
    if path.starts_with("/api/") || path.starts_with("/ws/") {
        return Err(actix_web::error::ErrorNotFound("Not Found"));
    }
    
    // For all other unmatched routes, serve the index file
    let index_path = config.root_path.join(&config.index_file);
    Ok(NamedFile::open(index_path)?)
}

// Configure static file serving with SPA support
pub fn configure(cfg: &mut web::ServiceConfig, config: StaticFilesConfig) {
    // Store config in app data for handlers
    let config_data = web::Data::new(config.clone());
    
    tracing::info!("Configuring static file serving from: {:?}", config.root_path);
    
    if config.enable_compression {
        tracing::info!("File compression enabled");
    } else {
        tracing::info!("File compression disabled");
    }
    
    tracing::info!("Cache-Control: {}", build_cache_control_value(&config.cache_control));
    
    // Add app data for the config
    cfg.app_data(config_data.clone());
    
    // Configure services differently based on compression setting
    if config.enable_compression {
        // With compression
        cfg.service(
            web::scope("")
                .wrap(Compress::default())
                .wrap(
                    actix_web::middleware::DefaultHeaders::new()
                        .add((header::CACHE_CONTROL, build_cache_control_value(&config.cache_control)))
                )
                .service(
                    Files::new("/", &config.root_path)
                        .index_file(&config.index_file)
                        .prefer_utf8(true)
                        .use_etag(true)
                        .use_last_modified(true)
                )
        );
    } else {
        // Without compression
        cfg.service(
            web::scope("")
                .wrap(
                    actix_web::middleware::DefaultHeaders::new()
                        .add((header::CACHE_CONTROL, build_cache_control_value(&config.cache_control)))
                )
                .service(
                    Files::new("/", &config.root_path)
                        .index_file(&config.index_file)
                        .prefer_utf8(true)
                        .use_etag(true)
                        .use_last_modified(true)
                )
        );
    }
    
    // Add a catch-all route for SPA support
    cfg.default_service(web::route().to(spa_index));
}

// Helper function to get static file config from environment
pub fn get_static_config() -> StaticFilesConfig {
    StaticFilesConfig::from_env()
}