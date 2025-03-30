// WebSocket Server - actors/client_session_actor.rs
// my-actix-system/websocket-server/src/actors/client_session_actor.rs
use actix::{Actor, AsyncContext, StreamHandler};
use actix_web_actors::ws;
use common::ClientMessage;
use uuid::Uuid;

/// Actor managing a client WebSocket connection
pub struct ClientSessionActor {
    client_id: Uuid,
    authenticated: bool,
    wallet_address: Option<String>,
}

impl ClientSessionActor {
    pub fn new(client_id: Uuid) -> Self {
        Self {
            client_id,
            authenticated: false,
            wallet_address: None,
        }
    }
    
    pub fn with_auth(client_id: Uuid, wallet_address: String) -> Self {
        Self {
            client_id,
            authenticated: true,
            wallet_address: Some(wallet_address),
        }
    }
}

impl Actor for ClientSessionActor {
    type Context = ws::WebsocketContext<Self>;
    
    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!("Client connected: {}", self.client_id);
        
        // Setup heartbeat
        self.heartbeat(ctx);
    }
    
    fn stopped(&mut self, _ctx: &mut Self::Context) {
        tracing::info!("Client disconnected: {}", self.client_id);
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for ClientSessionActor {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(msg)) => ctx.pong(&msg),
            Ok(ws::Message::Text(text)) => {
                // Basic echo for testing in Phase 1
                tracing::info!("Received message from client {}: {}", self.client_id, text);
                
                // For Phase 1, create a ClientMessage and echo it back
                let client_msg = ClientMessage {
                    client_id: self.client_id,
                    content: text.to_string(),
                    authenticated: self.authenticated,
                    wallet_address: self.wallet_address.clone(),
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                };
                
                // Serialize and echo back in Phase 1
                match serde_json::to_string(&client_msg) {
                    Ok(json) => ctx.text(json),
                    Err(e) => {
                        tracing::error!("Failed to serialize client message: {}", e);
                        ctx.text(format!("Error: Message serialization failed"));
                    }
                }
            },
            _ => (),
        }
    }
}

impl ClientSessionActor {
    fn heartbeat(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(std::time::Duration::from_secs(30), |_act, ctx| {
            ctx.ping(b"");
        });
    }
}