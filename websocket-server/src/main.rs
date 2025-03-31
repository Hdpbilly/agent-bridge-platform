// websocket-server/src/main.rs
// WebSocket Server - main.rs

mod actors;
mod routing;

use actix_web::{web, App, HttpServer};
use actors::state_manager::StateManagerActor;
use actors::router_actor::RouterActor;
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
    
    // Initialize the router actor
    let router = RouterActor::new().start();
    
    // Initialize the state manager actor
    let state_manager = StateManagerActor::new().start();
    
    // Make state manager aware of router
    state_manager.do_send(actors::state_manager::SetRouter {
        router: router.clone(),
    });
    
    tracing::info!("Starting WebSocket Server on {}", server_addr);
    
    // Create data references
    let config_data = web::Data::new(config);
    let router_data = web::Data::new(router);
    let state_manager_data = web::Data::new(state_manager.clone());
    
    // Start HTTP server
    HttpServer::new(move || {
        App::new()
            .app_data(state_manager_data.clone())
            .app_data(router_data.clone())
            .app_data(config_data.clone())
            .configure(routes)
    })
    .bind(&server_addr)?
    .run()
    .await
}