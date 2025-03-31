// websocket-server/src/actors/agent_actor.rs
use actix::{Actor, AsyncContext, StreamHandler, Context, Addr, Handler};
use actix_web_actors::ws;
use common::{AgentMessage, SystemMessage};
use std::time::{Duration, Instant, SystemTime};
use uuid::Uuid;
use super::state_manager::{
    StateManagerActor, UnregisterAgent, ConnectionState,
    UpdateAgentState, AgentActivity
};
use super::router_actor::AgentActorMessage;

// Enhanced agent actor
pub struct AgentActor {
    id: String,
    token: String,
    state_manager: Option<Addr<StateManagerActor>>,
    last_heartbeat: Instant,
    heartbeat_interval: Duration,
    heartbeat_timeout: Duration, 
    reconnect_attempts: u32,
    message_buffer: Vec<AgentMessage>,
}

impl AgentActor {
    pub fn new(id: String, token: String) -> Self {
        Self { 
            id, 
            token, 
            state_manager: None,
            last_heartbeat: Instant::now(),
            heartbeat_interval: Duration::from_secs(5),
            heartbeat_timeout: Duration::from_secs(30),
            reconnect_attempts: 0,
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
                tracing::warn!("Agent heartbeat timeout: {}", act.id);
                
                // Update state manager about reconnecting status
                if let Some(state_manager) = &act.state_manager {
                    state_manager.do_send(UpdateAgentState {
                        agent_id: act.id.clone(),
                        state: ConnectionState::Reconnecting,
                        last_seen_update: true,
                    });
                }
                
                // Calculate backoff duration (1s, 2s, 4s, 8s... capped at 60s)
                let backoff_seconds = std::cmp::min(
                    2u64.pow(act.reconnect_attempts),
                    60
                );
                let backoff = Duration::from_secs(backoff_seconds);
                
                // Log reconnection attempt
                tracing::info!(
                    "Agent {} reconnection attempt {} scheduled in {} seconds", 
                    act.id, act.reconnect_attempts + 1, backoff_seconds
                );
                
                // Attempt reconnection with backoff
                ctx.run_later(backoff, |act, ctx| {
                    // Reset heartbeat for reconnection attempt
                    act.last_heartbeat = Instant::now();
                    
                    // Increment reconnection counter
                    act.reconnect_attempts += 1;
                    
                    // For Phase 2, we just send a ping to check connectivity
                    ctx.ping(b"reconnect_attempt");
                });
            } else {
                // Send regular ping
                ctx.ping(b"");
            }
        });
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
    
    // Send buffered messages
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
    
    // Update activity with state manager
    fn update_activity(&self, is_message: bool) {
        if let Some(state_manager) = &self.state_manager {
            state_manager.do_send(AgentActivity {
                agent_id: self.id.clone(),
                is_message,
            });
        }
    }
}

impl Actor for AgentActor {
    type Context = ws::WebsocketContext<Self>;
    
    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!("Agent connected: {}", self.id);
        
        // Reset connection state
        self.last_heartbeat = Instant::now();
        
        // Reset reconnect attempts on successful connection
        self.reconnect_attempts = 0;
        
        // Setup heartbeat
        self.heartbeat(ctx);
        
        // Send any buffered messages
        self.send_buffered_messages(ctx);
        
        // Notify state manager about connection if set
        if let Some(state_manager) = &self.state_manager {
            state_manager.do_send(UpdateAgentState {
                agent_id: self.id.clone(),
                state: ConnectionState::Connected,
                last_seen_update: true,
            });
        }
    }
    
    fn stopped(&mut self, _ctx: &mut Self::Context) {
        tracing::info!("Agent disconnected: {}", self.id);
        
        // Notify state manager about disconnection
        if let Some(state_manager) = &self.state_manager {
            state_manager.do_send(UpdateAgentState {
                agent_id: self.id.clone(),
                state: ConnectionState::Disconnected,
                last_seen_update: true,
            });
            
            state_manager.do_send(UnregisterAgent {
                agent_id: self.id.clone(),
            });
        }
    }
}

impl Handler<AgentActorMessage> for AgentActor {
    type Result = ();
    
    fn handle(&mut self, msg: AgentActorMessage, ctx: &mut Self::Context) -> Self::Result {
        // Update last heartbeat on message
        self.last_heartbeat = Instant::now();
        
        // Update activity (is a message)
        self.update_activity(true);
        
        // Forward the message
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
                
                // Update activity (not a message)
                self.update_activity(false);
                
                // Reset reconnect attempts on successful ping
                if self.reconnect_attempts > 0 {
                    tracing::info!("Agent {} reconnected successfully", self.id);
                    self.reconnect_attempts = 0;
                    
                    // Update state manager if available
                    if let Some(state_manager) = &self.state_manager {
                        state_manager.do_send(UpdateAgentState {
                            agent_id: self.id.clone(),
                            state: ConnectionState::Connected,
                            last_seen_update: true,
                        });
                    }
                }
            },
            Ok(ws::Message::Pong(_)) => {
                // Update last heartbeat time on pong response
                self.last_heartbeat = Instant::now();
                
                // Update activity (not a message)
                self.update_activity(false);
                
                // Reset reconnect attempts on successful pong (could be response to reconnection ping)
                if self.reconnect_attempts > 0 {
                    tracing::info!("Agent {} reconnected successfully via pong", self.id);
                    self.reconnect_attempts = 0;
                    
                    // Update state manager if available
                    if let Some(state_manager) = &self.state_manager {
                        state_manager.do_send(UpdateAgentState {
                            agent_id: self.id.clone(),
                            state: ConnectionState::Connected,
                            last_seen_update: true,
                        });
                    }
                }
            },
            Ok(ws::Message::Text(text)) => {
                // Update last heartbeat time on any message
                self.last_heartbeat = Instant::now();
                
                // Update activity (is a message)
                self.update_activity(true);
                
                // Process agent message
                tracing::info!("Received message from agent {}: {}", self.id, text);
                
                // Try to parse as AgentMessage
                match serde_json::from_str::<AgentMessage>(&text) {
                    Ok(agent_msg) => {
                        tracing::info!("Parsed agent message: {:?}", agent_msg);
                        
                        // In Phase 2, we'd forward to router
                        // For now, echo back for testing
                        ctx.text(text);
                    },
                    Err(e) => {
                        tracing::warn!("Failed to parse agent message: {}", e);
                        
                        // Try to create a minimal agent message
                        let fallback_msg = AgentMessage {
                            target_client_id: None, // Broadcast
                            content: format!("Invalid message format received from agent: {}", e),
                            timestamp: SystemTime::now()
                                .duration_since(SystemTime::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs(),
                        };
                        
                        if let Ok(json) = serde_json::to_string(&fallback_msg) {
                            ctx.text(json);
                        }
                    }
                }
            },
            Ok(ws::Message::Binary(_)) => {
                // Update last heartbeat time
                self.last_heartbeat = Instant::now();
                
                // Update activity (is a message)
                self.update_activity(true);
                
                // Binary messages not supported in Phase 2
                tracing::warn!("Binary messages not supported for agent: {}", self.id);
            },
            Ok(ws::Message::Close(reason)) => {
                tracing::info!("Agent closing connection: {:?}", reason);
                
                // Update state manager
                if let Some(state_manager) = &self.state_manager {
                    state_manager.do_send(UpdateAgentState {
                        agent_id: self.id.clone(),
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
                tracing::trace!("Received continuation frame from agent {}", self.id);
            },
            Ok(ws::Message::Nop) => {
                // No operation - update heartbeat but otherwise ignore
                self.last_heartbeat = Instant::now();
                tracing::trace!("Received nop frame from agent {}", self.id);
            },
            Err(e) => {
                tracing::error!("WebSocket protocol error for agent {}: {}", self.id, e);
                
                // Update state manager
                if let Some(state_manager) = &self.state_manager {
                    state_manager.do_send(UpdateAgentState {
                        agent_id: self.id.clone(),
                        state: ConnectionState::Error,
                        last_seen_update: true,
                    });
                }
            }
        }
    }
}