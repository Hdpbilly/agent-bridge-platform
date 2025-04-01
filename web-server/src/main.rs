// web-server/src/main.rs - modified version
mod proxy;
mod auth;
mod static_files; // Add the new module

use actix_web::{web, App, HttpServer, Responder, HttpResponse, get};
use common::{setup_tracing, Config};
use uuid::Uuid;

#[get("/api")]
async fn api_index() -> impl Responder {
    HttpResponse::Ok().body("Agent Bridge Platform API")
}

#[get("/api/client")]
async fn create_client() -> impl Responder {
    // For Phase 1, just generate a new UUID for anonymous client
    let client_id = Uuid::new_v4();
    
    HttpResponse::Ok().json(serde_json::json!({
        "client_id": client_id
    }))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Setup tracing
    setup_tracing();
    
    // Load configuration
    let config = Config::from_env();
    
    // Save address before moving config into web::Data
    let server_addr = config.web_server_addr.clone();
    
    // Get static files configuration
    let static_config = static_files::get_static_config();
    
    tracing::info!("Starting Web Server on {}", server_addr);
    tracing::info!("Serving static files from: {:?}", static_config.root_path);
    
    // Create data reference
    let config_data = web::Data::new(config);
    
    // Start HTTP server
    HttpServer::new(move || {
        App::new()
            .app_data(config_data.clone())
            // API endpoints under /api prefix
            .service(api_index)
            .service(create_client)
            .configure(proxy::configure)
            // Static files serving with SPA support
            .configure(|cfg| {
                static_files::configure(cfg, static_config.clone());
            })
    })
    .bind(&server_addr)?
    .run()
    .await
}