// web-server/src/main.rs
mod proxy;
mod auth;
mod static_files;
mod api;
mod client_registry;
mod middleware;
mod utils;

use actix::Actor;
use actix_web::{web, App, HttpServer, middleware::{Compress, Logger}};
use common::{setup_tracing, Config};
use client_registry::ClientRegistryActor;
use middleware::RateLimiter;

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
    
    // Initialize ClientRegistryActor with a 24-hour session TTL
    let client_registry = ClientRegistryActor::new()
        .with_ttl(86400) // 24 hours in seconds
        .with_cleanup_interval(3600) // Clean up expired sessions every hour
        .start();
    tracing::info!("ClientRegistryActor started");
    
    // Log cache and compression settings
    if static_config.enable_compression {
        tracing::info!("Static assets compression: enabled");
    } else {
        tracing::info!("Static assets compression: disabled");
    }
    
    let cache_info = format!("Cache-Control: max-age={}", static_config.cache_control.max_age);
    tracing::info!("{}", cache_info);
    
    // Create rate limiter for client creation endpoint
    let client_rate_limiter = RateLimiter::new(vec!["/api/client".to_string()]);
    tracing::info!("Rate limiter configured for /api/client endpoint");
    
    // Create data references
    let config_data = web::Data::new(config);
    let client_registry_data = web::Data::new(client_registry);
    let static_config_clone = static_config.clone();
    
    // Start HTTP server with conditional configuration based on compression setting
    if static_config.enable_compression {
        // With compression
        HttpServer::new(move || {
            App::new()
                .app_data(config_data.clone())
                .app_data(client_registry_data.clone())
                .wrap(Logger::default())
                .wrap(client_rate_limiter.clone())
                .wrap(Compress::default())
                .configure(api::configure)
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
                .app_data(client_registry_data.clone())
                .wrap(Logger::default())
                .wrap(client_rate_limiter.clone())
                .configure(api::configure)
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