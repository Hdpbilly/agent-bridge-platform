// web-server/src/main.rs
mod proxy;
mod auth;
mod static_files;

use actix_web::{web, App, HttpServer, Responder, HttpResponse, get, middleware::Compress};
use common::{setup_tracing, Config};
use uuid::Uuid;

#[get("/api")]
async fn api_index() -> impl Responder {
    HttpResponse::Ok().body("Agent Bridge Platform API")
}

#[get("/api/client")]
async fn create_client() -> impl Responder {
    // Generate a new UUID for anonymous client
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
    
    // Log cache and compression settings
    if static_config.enable_compression {
        tracing::info!("Static assets compression: enabled");
    } else {
        tracing::info!("Static assets compression: disabled");
    }
    
    let cache_info = format!("Cache-Control: max-age={}", static_config.cache_control.max_age);
    tracing::info!("{}", cache_info);
    
    // Create data reference
    let config_data = web::Data::new(config);
    let static_config_clone = static_config.clone();
    
    // Start HTTP server with conditional configuration based on compression setting
    if static_config.enable_compression {
        // With compression
        HttpServer::new(move || {
            App::new()
                .app_data(config_data.clone())
                .wrap(Compress::default())
                .service(api_index)
                .service(create_client)
                .configure(proxy::configure)
                .configure(|cfg| {
                    static_files::configure(cfg, static_config_clone.clone());
                })
        })
        .bind(&server_addr)?
        .run()
        .await
    } else {
        // Without compression
        HttpServer::new(move || {
            App::new()
                .app_data(config_data.clone())
                .service(api_index)
                .service(create_client)
                .configure(proxy::configure)
                .configure(|cfg| {
                    static_files::configure(cfg, static_config_clone.clone());
                })
        })
        .bind(&server_addr)?
        .run()
        .await
    }
}