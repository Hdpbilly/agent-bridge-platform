// websocket-server/src/actors/client_session_actor.rs
use actix::{Actor, ActorContext, AsyncContext, StreamHandler, Addr, Context, Handler};
use actix_web_actors::ws;
use common::{ClientMessage, SystemMessage};
use uuid::Uuid;
use std::time::{Duration, Instant};
use super::state_manager::{
    StateManagerActor, UnregisterClient, ConnectionState, 
    UpdateClientState, ClientActivity
};
use super::router_actor::ClientActorMessage;

// Enhanced client session actor
pub struct ClientSessionActor {
    client_id: Uuid,
    authenticated: bool,
    wallet_address: Option<String>,
    state_manager: Option<Addr<StateManagerActor>>,
    last_heartbeat: Instant,
    heartbeat_interval: Duration,
    heartbeat_timeout: Duration,
    message_buffer: Vec<String>,
}

impl ClientSessionActor {
    pub fn new(client_id: Uuid) -> Self {
        Self {
            client_id,
            authenticated: false,
            wallet_address: None,
            state_manager: None,
            last_heartbeat: Instant::now(),
            heartbeat_interval: Duration::from_secs(5),
            heartbeat_timeout: Duration::from_secs(30),
            message_buffer: Vec::new(),
        }
    }
    
    pub fn with_auth(client_id: Uuid, wallet_address: String) -> Self {
        Self {
            client_id,
            authenticated: true,
            wallet_address: Some(wallet_address),
            state_manager: None,
            last_heartbeat: Instant::now(),
            heartbeat_interval: Duration::from_secs(5),
            heartbeat_timeout: Duration::from_secs(30),
            message_buffer: Vec::new(),
        }
    }
    
    // Set state manager reference
    pub fn set_state_manager(&mut self, addr: Addr<StateManagerActor>) {
        self.state_manager = Some(addr);
    }
    
    // Enhanced heartbeat with timeout detection
    fn heartbeat(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(self.heartbeat_interval, |act, ctx| {
            // Check for heartbeat timeout
            if Instant::now().duration_since(act.last_heartbeat) > act.heartbeat_timeout {
                tracing::warn!("Client heartbeat timeout: {}", act.client_id);
                
                // Update state manager
                if let Some(state_manager) = &act.state_manager {
                    // Update to disconnected state
                    state_manager.do_send(UpdateClientState {
                        client_id: act.client_id,
                        state: ConnectionState::Disconnected,
                        last_seen_update: true,
                    });
                    
                    // Unregister from state manager
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
    
    // Update activity with state manager
    fn update_activity(&self, is_message: bool) {
        if let Some(state_manager) = &self.state_manager {
            state_manager.do_send(ClientActivity {
                client_id: self.client_id,
                is_message,
            });
        }
    }
}

impl Actor for ClientSessionActor {
    type Context = ws::WebsocketContext<Self>;
    
    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!("Client connected: {}", self.client_id);
        
        // Reset heartbeat
        self.last_heartbeat = Instant::now();
        
        // Setup heartbeat
        self.heartbeat(ctx);
        
        // Send any buffered messages
        self.send_buffered_messages(ctx);
    }
    
    fn stopped(&mut self, _ctx: &mut Self::Context) {
        tracing::info!("Client disconnected: {}", self.client_id);
        
        // Notify state manager about disconnection (if not already done in heartbeat)
        if let Some(state_manager) = &self.state_manager {
            state_manager.do_send(UpdateClientState {
                client_id: self.client_id,
                state: ConnectionState::Disconnected,
                last_seen_update: true,
            });
            
            state_manager.do_send(UnregisterClient {
                client_id: self.client_id,
            });
        }
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for ClientSessionActor {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(msg)) => {
                // Update last heartbeat time
                self.last_heartbeat = Instant::now();
                ctx.pong(&msg);
                
                // Update activity (not a message)
                self.update_activity(false);
            },
            Ok(ws::Message::Pong(_)) => {
                // Update last heartbeat time on pong response
                self.last_heartbeat = Instant::now();
                
                // Update activity (not a message)
                self.update_activity(false);
            },
            Ok(ws::Message::Text(text)) => {
                // Update last heartbeat time on any message
                self.last_heartbeat = Instant::now();
                
                // Update activity (is a message)
                self.update_activity(true);
                
                // Process client message
                tracing::info!("Received message from client {}: {}", self.client_id, text);
                
                // Create a ClientMessage
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
            Ok(ws::Message::Binary(_)) => {
                // Update last heartbeat time
                self.last_heartbeat = Instant::now();
                
                // Update activity (is a message)
                self.update_activity(true);
                
                // Binary messages not supported in Phase 2
                ctx.text("Binary messages not supported");
            },
            Ok(ws::Message::Close(reason)) => {
                tracing::info!("Client closing connection: {:?}", reason);
                
                // Update state manager
                if let Some(state_manager) = &self.state_manager {
                    state_manager.do_send(UpdateClientState {
                        client_id: self.client_id,
                        state: ConnectionState::Disconnected,
                        last_seen_update: true,
                    });
                }
                
                ctx.close(reason);
            },
            // Add handlers for the missing message types
            Ok(ws::Message::Continuation(_)) => {
                // Actix-web handles continuations automatically for most use cases
                // Just update heartbeat and log at trace level
                self.last_heartbeat = Instant::now();
                tracing::trace!("Received continuation frame from client {}", self.client_id);
            },
            Ok(ws::Message::Nop) => {
                // No operation - update heartbeat but otherwise ignore
                self.last_heartbeat = Instant::now();
                tracing::trace!("Received nop frame from client {}", self.client_id);
            },
            Err(e) => {
                tracing::error!("WebSocket protocol error from client {}: {}", self.client_id, e);
                
                // Update state manager on error
                if let Some(state_manager) = &self.state_manager {
                    state_manager.do_send(UpdateClientState {
                        client_id: self.client_id,
                        state: ConnectionState::Error,
                        last_seen_update: true,
                    });
                }
            }
        }
    }
}

impl Handler<ClientActorMessage> for ClientSessionActor {
    type Result = ();
    
    fn handle(&mut self, msg: ClientActorMessage, ctx: &mut Self::Context) -> Self::Result {
        // Forward the message to the client
        ctx.text(msg.content);
    }
}