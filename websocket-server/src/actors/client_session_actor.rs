// WebSocket Server - actors/client_session_actor.rs
// websocket-server/src/actors/client_session_actor.rs
use actix::{Actor, ActorContext, AsyncContext, StreamHandler, Addr, Context, Handler};
use actix_web_actors::ws;
use common::{ClientMessage, SystemMessage};
use uuid::Uuid;
use std::time::{Duration, Instant};
use super::state_manager::{StateManagerActor, UnregisterClient};
use super::router_actor::ClientActorMessage;

// Connection state tracking
enum ConnectionState {
    Connected,
    Disconnected,
}

/// Enhanced actor managing a client WebSocket connection
pub struct ClientSessionActor {
    client_id: Uuid,
    authenticated: bool,
    wallet_address: Option<String>,
    state: ConnectionState,
    last_heartbeat: Instant,
    state_manager: Option<Addr<StateManagerActor>>,
    message_buffer: Vec<String>,
}

impl ClientSessionActor {
    pub fn new(client_id: Uuid) -> Self {
        Self {
            client_id,
            authenticated: false,
            wallet_address: None,
            state: ConnectionState::Connected,
            last_heartbeat: Instant::now(),
            state_manager: None,
            message_buffer: Vec::new(),
        }
    }
    
    pub fn with_auth(client_id: Uuid, wallet_address: String) -> Self {
        Self {
            client_id,
            authenticated: true,
            wallet_address: Some(wallet_address),
            state: ConnectionState::Connected,
            last_heartbeat: Instant::now(),
            state_manager: None,
            message_buffer: Vec::new(),
        }
    }
    
    // Set state manager reference
    pub fn set_state_manager(&mut self, addr: Addr<StateManagerActor>) {
        self.state_manager = Some(addr);
    }
    
    // Enhanced heartbeat with timeout detection
    fn heartbeat(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(Duration::from_secs(5), |act, ctx| {
            // Check for heartbeat timeout (30 seconds)
            if Instant::now().duration_since(act.last_heartbeat) > Duration::from_secs(30) {
                tracing::warn!("Client heartbeat timeout: {}", act.client_id);
                
                // Update the state to disconnected
                act.state = ConnectionState::Disconnected;
                
                // Notify state manager about disconnection
                if let Some(state_manager) = &act.state_manager {
                    state_manager.do_send(UnregisterClient {
                        client_id: act.client_id,
                    });
                }
                
                // Stop actor
                ctx.stop();
                return;
            }
            
            // Send ping
            ctx.ping(b"");
        });
    }
    
    // Buffer a message for later delivery
    pub fn buffer_message(&mut self, message: String) {
        // Limit buffer size to 100 messages
        if self.message_buffer.len() < 100 {
            self.message_buffer.push(message);
        } else {
            tracing::warn!("Message buffer full for client: {}, dropping message", self.client_id);
        }
    }
    
    // Send buffered messages
    fn send_buffered_messages(&mut self, ctx: &mut ws::WebsocketContext<Self>) {
        if !self.message_buffer.is_empty() {
            tracing::info!("Sending {} buffered messages for client: {}", 
                          self.message_buffer.len(), self.client_id);
                          
            for msg in self.message_buffer.drain(..) {
                ctx.text(msg);
            }
        }
    }
}

impl Actor for ClientSessionActor {
    type Context = ws::WebsocketContext<Self>;
    
    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!("Client connected: {}", self.client_id);
        
        // Reset state
        self.state = ConnectionState::Connected;
        self.last_heartbeat = Instant::now();
        
        // Setup heartbeat
        self.heartbeat(ctx);
        
        // Send any buffered messages
        self.send_buffered_messages(ctx);
        
        // Notify about client connection through system message
        // This would normally go to the router in a full implementation
        let system_msg = SystemMessage::ClientConnected {
            client_id: self.client_id,
            authenticated: self.authenticated,
            wallet_address: self.wallet_address.clone(),
        };
        
        tracing::info!("Client connection system message: {:?}", system_msg);
    }
    
    fn stopped(&mut self, _ctx: &mut Self::Context) {
        tracing::info!("Client disconnected: {}", self.client_id);
        
        // Update state
        self.state = ConnectionState::Disconnected;
        
        // Notify state manager about disconnection (if not already done in heartbeat)
        if let Some(state_manager) = &self.state_manager {
            state_manager.do_send(UnregisterClient {
                client_id: self.client_id,
            });
        }
        
        // Notify about client disconnection through system message
        let system_msg = SystemMessage::ClientDisconnected {
            client_id: self.client_id,
        };
        
        tracing::info!("Client disconnection system message: {:?}", system_msg);
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for ClientSessionActor {
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
                
                // Process client message
                tracing::info!("Received message from client {}: {}", self.client_id, text);
                
                // For Phase 2, create a ClientMessage and route to the agent
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
                
                // In a full implementation, this would be sent to the RouterActor
                // For now, just echo back
                match serde_json::to_string(&client_msg) {
                    Ok(json) => ctx.text(json),
                    Err(e) => {
                        tracing::error!("Failed to serialize client message: {}", e);
                        ctx.text(format!("Error: Message serialization failed"));
                    }
                }
            },
            Ok(ws::Message::Close(reason)) => {
                tracing::info!("Client closing connection: {:?}", reason);
                ctx.close(reason);
            },
            _ => (),
        }
    }
}

impl Handler<ClientActorMessage> for ClientSessionActor {
    type Result = ();
    
    fn handle(&mut self, msg: ClientActorMessage, ctx: &mut Self::Context) -> Self::Result {
        // Simply forward the message to the client
        ctx.text(msg.content);
    }
}