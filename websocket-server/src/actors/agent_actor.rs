// WebSocket Server - actors/agent_actor.rs
// my-actix-system/websocket-server/src/actors/agent_actor.rs
use actix::{Actor, AsyncContext, StreamHandler};
use actix_web_actors::ws;
use common::AgentMessage;

/// Actor managing the connection with the Agent Runtime
pub struct AgentActor {
    id: String,
    token: String,
}

impl AgentActor {
    pub fn new(id: String, token: String) -> Self {
        Self { id, token }
    }
}

impl Actor for AgentActor {
    type Context = ws::WebsocketContext<Self>;
    
    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!("Agent connected: {}", self.id);
        
        // Setup heartbeat
        self.heartbeat(ctx);
    }
    
    fn stopped(&mut self, _ctx: &mut Self::Context) {
        tracing::info!("Agent disconnected: {}", self.id);
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for AgentActor {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(msg)) => ctx.pong(&msg),
            Ok(ws::Message::Text(text)) => {
                // Basic echo for testing in Phase 1
                tracing::info!("Received message from agent: {}", text);
                
                // Try to parse as AgentMessage
                match serde_json::from_str::<AgentMessage>(&text) {
                    Ok(agent_msg) => {
                        tracing::info!("Parsed agent message: {:?}", agent_msg);
                        // In Phase 1, just echo back
                        ctx.text(text);
                    },
                    Err(e) => {
                        tracing::warn!("Failed to parse agent message: {}", e);
                        ctx.text(format!("Error: Invalid message format"));
                    }
                }
            },
            _ => (),
        }
    }
}

impl AgentActor {
    fn heartbeat(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(std::time::Duration::from_secs(30), |_act, ctx| {
            ctx.ping(b"");
        });
    }
}