// websocket-server/src/routing.rs
use actix_web::{web, HttpRequest, HttpResponse, Error};
use actix_web_actors::ws;
use actix::Addr;
use common::Config;
use uuid::Uuid;

use crate::actors::{
    agent_actor::AgentActor,
    client_session_actor::ClientSessionActor,
    state_manager::{StateManagerActor, RegisterClient, RegisterAgent},
};

/// Configure routes for the WebSocket server
pub fn routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::resource("/ws/agent")
            .route(web::get().to(agent_ws_route))
    ).service(
        web::resource("/ws/client/{client_id}")
            .route(web::get().to(client_ws_route))
    );
}

/// WebSocket route for agent connections
async fn agent_ws_route(
    req: HttpRequest,
    stream: web::Payload,
    state_manager: web::Data<Addr<StateManagerActor>>,
    config: web::Data<Config>,
) -> Result<HttpResponse, Error> {
    // Extract token from headers
    let auth_header = req.headers().get("Authorization");
    let token = match auth_header {
        Some(header) => header.to_str().unwrap_or_default(),
        None => {
            tracing::warn!("Agent connection attempt without Authorization header");
            return Ok(HttpResponse::Unauthorized().finish());
        },
    };
    
    // Validate token (simple comparison for Phase 2)
    if token != config.agent_token {
        tracing::warn!("Agent connection attempt with invalid token");
        return Ok(HttpResponse::Unauthorized().finish());
    }
    
    // Create agent actor
    let agent_id = "agent1".to_string(); // Hardcoded for Phase 2
    let mut agent = AgentActor::new(agent_id.clone(), token.to_string());
    
    // Set state manager
    agent.set_state_manager(state_manager.get_ref().clone());
    
    // Start WebSocket connection with callback to capture actor address
    ws::start_with_addr(agent, &req, stream).map(|(addr, resp)| {
        // Register agent with state manager using the actor address
        state_manager.do_send(RegisterAgent {
            agent_id,
            addr,
        });
        
        // Return the HTTP response
        resp
    })
}

/// WebSocket route for client connections
async fn client_ws_route(
    req: HttpRequest,
    stream: web::Payload,
    state_manager: web::Data<Addr<StateManagerActor>>,
    path: web::Path<(String,)>,
) -> Result<HttpResponse, Error> {
    // Extract client_id from path
    let client_id_str = &path.0;
    let client_id = match Uuid::parse_str(client_id_str) {
        Ok(id) => id,
        Err(_) => {
            tracing::warn!("Invalid client ID format in WebSocket connection: {}", client_id_str);
            return Ok(HttpResponse::BadRequest().finish());
        },
    };
    
    // Create client session actor
    let mut client = ClientSessionActor::new(client_id);
    
    // Set state manager
    client.set_state_manager(state_manager.get_ref().clone());
    
    // Start WebSocket connection with callback to capture actor address
    ws::start_with_addr(client, &req, stream).map(|(addr, resp)| {
        // Register client with state manager using the actor address
        state_manager.do_send(RegisterClient {
            client_id,
            addr,
            authenticated: false, // Phase 2 - authentication not implemented yet
            wallet_address: None, // Phase 2 - no wallet address yet
        });
        
        // Return the HTTP response
        resp
    })
}