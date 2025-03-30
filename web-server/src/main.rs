// Web Server - main.rs
// my-actix-system/web-server/src/main.rs
mod proxy;
mod auth;

use actix_web::{web, App, HttpServer, Responder, HttpResponse, get};
use common::{setup_tracing, Config};
use uuid::Uuid;

#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().body("Agent Bridge Platform Web Server")
}

#[get("/client")]
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
    
    tracing::info!("Starting Web Server on {}", server_addr);
    
    // Create data reference
    let config_data = web::Data::new(config);
    
    // Start HTTP server
    HttpServer::new(move || {
        App::new()
            .app_data(config_data.clone())
            .service(index)
            .service(create_client)
            .configure(proxy::configure)
    })
    .bind(&server_addr)?
    .run()
    .await
}