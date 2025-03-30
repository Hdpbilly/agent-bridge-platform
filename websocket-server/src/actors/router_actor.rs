// WebSocket Server - actors/router_actor.rs
// my-actix-system/websocket-server/src/actors/router_actor.rs
use actix::{Actor, Context, Handler, Message};
use uuid::Uuid;

/// Message for the router
#[derive(Message)]
#[rtype(result = "()")]
pub struct RouteMessage {
    pub message: String,
    pub from_client_id: Option<Uuid>,
    pub to_client_id: Option<Uuid>, // None means broadcast
}

/// Actor for routing messages between clients and agents
pub struct RouterActor;

impl RouterActor {
    pub fn new() -> Self {
        Self
    }
}

impl Actor for RouterActor {
    type Context = Context<Self>;
}

impl Handler<RouteMessage> for RouterActor {
    type Result = ();
    
    fn handle(&mut self, msg: RouteMessage, _ctx: &mut Self::Context) -> Self::Result {
        // Basic routing logic - Phase 1 is just routing setup
        match (msg.from_client_id, msg.to_client_id) {
            (Some(from), None) => {
                // Client to Agent (broadcast)
                tracing::info!("Routing message from client {} to agent", from);
            },
            (None, Some(to)) => {
                // Agent to specific client
                tracing::info!("Routing message from agent to client {}", to);
            },
            (None, None) => {
                // Agent broadcast to all clients
                tracing::info!("Broadcasting message from agent to all clients");
            },
            _ => {
                // Unsupported for now (client to client)
                tracing::warn!("Unsupported routing pattern");
            }
        }
    }
}