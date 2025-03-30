// WebSocket Server - actors/agent_actor.rs
// my-actix-system/websocket-server/src/actors/agent_actor.rs
// websocket-server/src/actors/agent_actor.rs
use actix::{Actor, AsyncContext, StreamHandler, Context, Addr, Handler};
use actix_web_actors::ws;
use common::{AgentMessage, SystemMessage};
use std::time::{Duration, Instant};
use uuid::Uuid;
use super::router_actor::AgentActorMessage;

// Connection state tracking
enum ConnectionState {
    Connected,
    Disconnected,
    Reconnecting,
}

// Enhanced AgentActor with connection management
pub struct AgentActor {
    id: String,
    token: String,
    state: ConnectionState,
    last_heartbeat: Instant,
    message_buffer: Vec<AgentMessage>,
    reconnect_attempts: u32,
}

impl AgentActor {
    pub fn new(id: String, token: String) -> Self {
        Self { 
            id, 
            token, 
            state: ConnectionState::Connected,
            last_heartbeat: Instant::now(),
            message_buffer: Vec::new(),
            reconnect_attempts: 0,
        }
    }
    
    // Enhanced heartbeat with timeout detection
    fn heartbeat(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(Duration::from_secs(5), |act, ctx| {
            // Check for heartbeat timeout (30 seconds)
            if Instant::now().duration_since(act.last_heartbeat) > Duration::from_secs(30) {
                tracing::warn!("Agent heartbeat timeout: {}", act.id);
                
                // Update state to Reconnecting
                act.state = ConnectionState::Reconnecting;
                
                // Calculate backoff duration (1s, 2s, 4s, 8s... capped at 60s)
                let backoff = std::cmp::min(
                    Duration::from_secs(2u64.pow(act.reconnect_attempts)),
                    Duration::from_secs(60)
                );
                
                // Attempt reconnection with backoff
                ctx.run_later(backoff, |act, ctx| {
                    tracing::info!("Attempting reconnection for agent: {}", act.id);
                    
                    // Reset heartbeat
                    act.last_heartbeat = Instant::now();
                    
                    // Increment reconnection counter
                    act.reconnect_attempts += 1;
                });
            } else {
                // Send regular ping
                ctx.ping(b"");
            }
        });
    }
    
    // Send buffered messages if any
    fn send_buffered_messages(&mut self, ctx: &mut ws::WebsocketContext<Self>) {
        if !self.message_buffer.is_empty() {
            tracing::info!("Sending {} buffered messages for agent: {}", 
                          self.message_buffer.len(), self.id);
                          
            for msg in self.message_buffer.drain(..) {
                if let Ok(serialized) = serde_json::to_string(&msg) {
                    ctx.text(serialized);
                }
            }
        }
    }
    
    // Buffer a message for later delivery
    pub fn buffer_message(&mut self, message: AgentMessage) {
        // Limit buffer size to 100 messages
        if self.message_buffer.len() < 100 {
            self.message_buffer.push(message);
        } else {
            tracing::warn!("Message buffer full for agent: {}, dropping message", self.id);
        }
    }
}

impl Actor for AgentActor {
    type Context = ws::WebsocketContext<Self>;
    
    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!("Agent connected: {}", self.id);
        
        // Reset connection state
        self.state = ConnectionState::Connected;
        self.reconnect_attempts = 0;
        self.last_heartbeat = Instant::now();
        
        // Setup heartbeat
        self.heartbeat(ctx);
        
        // Send any buffered messages
        self.send_buffered_messages(ctx);
    }
    
    fn stopped(&mut self, _ctx: &mut Self::Context) {
        tracing::info!("Agent disconnected: {}", self.id);
        self.state = ConnectionState::Disconnected;
    }
}

impl Handler<AgentActorMessage> for AgentActor {
    type Result = ();
    
    fn handle(&mut self, msg: AgentActorMessage, ctx: &mut Self::Context) -> Self::Result {
        // Forward the message to the agent
        ctx.text(msg.content);
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for AgentActor {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(msg)) => {
                // Update last heartbeat time
                self.last_heartbeat = Instant::now();
                ctx.pong(&msg);
            },
            Ok(ws::Message::Pong(_)) => {
                // Update last heartbeat time on pong response
                self.last_heartbeat = Instant::now();
            },
            Ok(ws::Message::Text(text)) => {
                // Update last heartbeat time on any message
                self.last_heartbeat = Instant::now();
                
                // Process agent message
                tracing::info!("Received message from agent: {}", text);
                
                // Try to parse as AgentMessage
                match serde_json::from_str::<AgentMessage>(&text) {
                    Ok(agent_msg) => {
                        tracing::info!("Parsed agent message: {:?}", agent_msg);
                        // In Phase 2, we'd forward to router actor
                        // For now, just echo back
                        ctx.text(text);
                    },
                    Err(e) => {
                        tracing::warn!("Failed to parse agent message: {}", e);
                        ctx.text(format!("Error: Invalid message format"));
                    }
                }
            },
            Ok(ws::Message::Close(reason)) => {
                tracing::info!("Agent closing connection: {:?}", reason);
                self.state = ConnectionState::Disconnected;
                ctx.close(reason);
            },
            _ => (),
        }
    }
}