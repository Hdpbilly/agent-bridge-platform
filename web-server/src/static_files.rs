// web-server/src/static_files.rs
use actix_web::{web, HttpRequest, HttpResponse, Result, Error};
use actix_files::{Files, NamedFile};
use std::path::{Path, PathBuf};

// Configuration for static file serving
#[derive(Clone)]
pub struct StaticFilesConfig {
    pub root_path: PathBuf,
    pub index_file: String,
}

impl Default for StaticFilesConfig {
    fn default() -> Self {
        Self {
            root_path: PathBuf::from("./static"),
            index_file: "index.html".to_string(),
        }
    }
}

// Async handler function for SPA fallback
async fn spa_index(req: HttpRequest, config: web::Data<StaticFilesConfig>) -> Result<HttpResponse, Error> {
    // Don't serve index.html for API or WebSocket routes
    let path = req.path();
    if path.starts_with("/api/") || path.starts_with("/ws/") {
        return Ok(HttpResponse::NotFound().finish());
    }
    
    // For all other unmatched routes, serve the index file (SPA support)
    let index_path = config.root_path.join(&config.index_file);
    let file = NamedFile::open(index_path)?;
    Ok(file.into_response(&req))
}

// Configure static file serving with SPA support
pub fn configure(cfg: &mut web::ServiceConfig, config: StaticFilesConfig) {
    // Store config in app data
    let config_data = web::Data::new(config.clone());
    
    // Serve static files from the configured directory
    cfg.app_data(config_data.clone())
        .service(
            Files::new("/", &config.root_path)
                .index_file(&config.index_file)
                .prefer_utf8(true)
                .use_etag(true)
                .use_last_modified(true)
                // Skip the default handler - we'll add it at the app level
        )
        // Add a catch-all route for SPA support with the lowest priority
        .default_service(web::route().to(spa_index));
}

// Helper function to get static file config from environment
pub fn get_static_config() -> StaticFilesConfig {
    let root_path = std::env::var("STATIC_ASSETS_PATH")
        .unwrap_or_else(|_| "./static".to_string());
    
    StaticFilesConfig {
        root_path: PathBuf::from(root_path),
        index_file: "index.html".to_string(),
    }
}