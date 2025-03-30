// WebSocket Server - main.rs
// my-actix-system/websocket-server/src/main.rs

mod actors;
mod routing;

use actix_web::{web, App, HttpServer};
use actors::state_manager::StateManagerActor;
use common::{setup_tracing, Config};
use routing::routes;
use actix::Actor;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Setup tracing
    setup_tracing();
    
    // Load configuration
    let config = Config::from_env();
    
    // Save address before moving config into web::Data
    let server_addr = config.websocket_server_addr.clone();
    
    // Initialize the state manager actor
    let state_manager = StateManagerActor::new().start();
    
    tracing::info!("Starting WebSocket Server on {}", server_addr);
    
    // Create data references
    let config_data = web::Data::new(config);
    
    // Start HTTP server
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(state_manager.clone()))
            .app_data(config_data.clone())
            .configure(routes)
    })
    .bind(&server_addr)?
    .run()
    .await
}