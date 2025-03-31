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
    router_actor::RouterActor, // Import RouterActor
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
    router: web::Data<Addr<RouterActor>>, // <-- Get RouterActor address
    config: web::Data<Config>,
) -> Result<HttpResponse, Error> {
    let auth_header = req.headers().get("Authorization");
    let token = match auth_header {
        Some(header) => header.to_str().unwrap_or_default(),
        None => {
            tracing::warn!("Agent connection attempt without Authorization header");
            return Ok(HttpResponse::Unauthorized().finish());
        },
    };

    if token != config.agent_token {
        tracing::warn!("Agent connection attempt with invalid token");
        return Ok(HttpResponse::Unauthorized().finish());
    }

    let agent_id = "agent1".to_string(); // Hardcoded for Phase 2
    let mut agent = AgentActor::new(agent_id.clone(), token.to_string());

    // Inject dependencies
    agent.set_state_manager(state_manager.get_ref().clone());
    agent.set_router(router.get_ref().clone()); // <-- Inject Router address

    // Start WebSocket connection
    ws::start_with_addr(agent, &req, stream).map(|(addr, resp)| {
        // Register agent with state manager
        state_manager.do_send(RegisterAgent {
            agent_id,
            addr, // This addr is the Addr<AgentActor>
        });
        resp
    })
}

/// WebSocket route for client connections
async fn client_ws_route(
    req: HttpRequest,
    stream: web::Payload,
    state_manager: web::Data<Addr<StateManagerActor>>,
    router: web::Data<Addr<RouterActor>>, // <-- Get RouterActor address
    path: web::Path<(String,)>,
) -> Result<HttpResponse, Error> {
    let client_id_str = &path.0;
    let client_id = match Uuid::parse_str(client_id_str) {
        Ok(id) => id,
        Err(_) => {
            tracing::warn!("Invalid client ID format: {}", client_id_str);
            return Ok(HttpResponse::BadRequest().finish());
        },
    };

    let mut client = ClientSessionActor::new(client_id);

    // Inject dependencies
    client.set_state_manager(state_manager.get_ref().clone());
    client.set_router(router.get_ref().clone()); // <-- Inject Router address

    // Start WebSocket connection
    ws::start_with_addr(client, &req, stream).map(|(addr, resp)| {
        // Register client with state manager
        state_manager.do_send(RegisterClient {
            client_id,
            addr, // This addr is the Addr<ClientSessionActor>
            authenticated: false, // Phase 2 default
            wallet_address: None, // Phase 2 default
        });
        resp
    })
}