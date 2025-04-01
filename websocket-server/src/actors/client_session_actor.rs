// websocket-server/src/actors/client_session_actor.rs
use actix::{Actor, ActorContext, AsyncContext, StreamHandler, Addr, Handler};
use actix::ContextFutureSpawner; // Added missing trait import
use actix_web_actors::ws;
use common::{ClientMessage, SystemMessage, MessageAcknowledgement, AckStatus};
use uuid::Uuid;
use std::time::{Duration, Instant, SystemTime};
use std::collections::{VecDeque, HashMap};
use super::state_manager::{
    StateManagerActor, UnregisterClient, ConnectionState,
    UpdateClientState, ClientActivity, SessionState, SaveSessionState, GetSessionState,
    UpdateClientMessageMetrics
};
use super::router_actor::{ClientActorMessage, RouterActor};

// Message tracking structure for delivery confirmation
struct MessageTracker {
    last_sent_id: u64,
    last_received_id: u64,
    pending_acks: HashMap<u64, (String, Instant)>, // message_id -> (content, sent_time)
    ack_timeout: Duration,
}

impl MessageTracker {
    fn new() -> Self {
        Self {
            last_sent_id: 0,
            last_received_id: 0,
            pending_acks: HashMap::new(),
            ack_timeout: Duration::from_secs(30),
        }
    }
    
    fn next_id(&mut self) -> u64 {
        self.last_sent_id += 1;
        self.last_sent_id
    }
    
    fn add_pending(&mut self, msg_id: u64, content: String) {
        self.pending_acks.insert(msg_id, (content, Instant::now()));
    }
    
    fn confirm_delivery(&mut self, msg_id: u64) -> bool {
        self.pending_acks.remove(&msg_id).is_some()
    }
    
    // Check for expired acknowledgements and return list of expired message IDs
    fn check_expired(&self) -> Vec<u64> {
        let now = Instant::now();
        self.pending_acks.iter()
            .filter(|(_, (_, sent_time))| now.duration_since(*sent_time) > self.ack_timeout)
            .map(|(id, _)| *id)
            .collect()
    }
}

// Enhanced client session actor with session persistence
pub struct ClientSessionActor {
    client_id: Uuid,
    authenticated: bool,
    wallet_address: Option<String>,
    state_manager: Option<Addr<StateManagerActor>>,
    router: Option<Addr<RouterActor>>,
    last_heartbeat: Instant,
    heartbeat_interval: Duration,
    heartbeat_timeout: Duration,
    reconnect_interval: Duration,
    reconnect_attempts: u32,
    max_reconnect_attempts: u32,
    // Enhanced session state
    message_buffer: VecDeque<String>,
    max_buffer_size: usize,
    session_id: Option<String>, // Unique session identifier
    session_data: HashMap<String, String>, // Arbitrary session data
    // Message tracking for delivery confirmation
    message_tracker: MessageTracker,
    delivery_confirmation: bool, // Whether to use delivery confirmation
    is_connected: bool, // Added to track connection status
}

impl ClientSessionActor {
    pub fn new(client_id: Uuid) -> Self {
        Self {
            client_id,
            authenticated: false,
            wallet_address: None,
            state_manager: None,
            router: None,
            last_heartbeat: Instant::now(),
            heartbeat_interval: Duration::from_secs(5),
            heartbeat_timeout: Duration::from_secs(30),
            reconnect_interval: Duration::from_secs(5),
            reconnect_attempts: 0,
            max_reconnect_attempts: 5,
            message_buffer: VecDeque::with_capacity(100),
            max_buffer_size: 100,
            session_id: Some(format!("session-{}-{}", client_id, 
                                   SystemTime::now()
                                      .duration_since(SystemTime::UNIX_EPOCH)
                                      .unwrap_or_default()
                                      .as_secs())),
            session_data: HashMap::new(),
            message_tracker: MessageTracker::new(),
            delivery_confirmation: true, // Enable by default
            is_connected: false, // Initialize as not connected
        }
    }

    pub fn with_auth(client_id: Uuid, wallet_address: String) -> Self {
        let mut actor = Self::new(client_id);
        actor.authenticated = true;
        actor.wallet_address = Some(wallet_address);
        actor
    }

    pub fn set_state_manager(&mut self, addr: Addr<StateManagerActor>) {
        self.state_manager = Some(addr);
    }

    pub fn set_router(&mut self, addr: Addr<RouterActor>) {
        self.router = Some(addr);
    }

    // Enhanced heartbeat with reconnection attempts
    fn heartbeat(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(self.heartbeat_interval, |act, ctx| {
            if Instant::now().duration_since(act.last_heartbeat) > act.heartbeat_timeout {
                tracing::warn!("Client heartbeat timeout: {}", act.client_id);

                // Save session state before attempting reconnection
                act.save_session_state();

                if let Some(state_manager) = &act.state_manager {
                    // Update state to reconnecting
                    state_manager.do_send(UpdateClientState {
                        client_id: act.client_id,
                        state: ConnectionState::Reconnecting,
                        last_seen_update: true,
                    });
                }
                
                // Check for expired message acknowledgements
                act.check_and_resend_pending_messages(ctx);
                
                // Increment reconnect attempts
                act.reconnect_attempts += 1;
                
                if act.reconnect_attempts > act.max_reconnect_attempts {
                    tracing::error!(
                        "Client {} exceeded maximum reconnection attempts ({}), stopping", 
                        act.client_id, act.max_reconnect_attempts
                    );
                    
                    // Update state to disconnected before stopping
                    if let Some(state_manager) = &act.state_manager {
                        state_manager.do_send(UpdateClientState {
                            client_id: act.client_id,
                            state: ConnectionState::Disconnected,
                            last_seen_update: true,
                        });
                        state_manager.do_send(UnregisterClient {
                            client_id: act.client_id,
                        });
                    }
                    
                    ctx.stop();
                    return;
                }
                
                tracing::info!(
                    "Client {} reconnection attempt {}/{}", 
                    act.client_id, act.reconnect_attempts, act.max_reconnect_attempts
                );
                
                // Try to ping again for reconnection
                ctx.ping(b"reconnect");
            } else {
                // Check for expired message acknowledgements during normal operation
                act.check_and_resend_pending_messages(ctx);
                
                // Send regular ping
                ctx.ping(b"");
            }
        });
    }

    // Buffer a message for later delivery
    pub fn buffer_message(&mut self, content: String) -> Option<u64> {
        let buffer_full = self.message_buffer.len() >= self.max_buffer_size;
        
        if buffer_full {
            tracing::warn!("Message buffer full for client: {}, dropping message", self.client_id);
            None
        } else {
            // If delivery confirmation is enabled, track the message
            let message_id = if self.delivery_confirmation {
                let id = self.message_tracker.next_id();
                self.message_tracker.add_pending(id, content.clone());
                Some(id)
            } else {
                None
            };
            
            self.message_buffer.push_back(content);
            
            // Update metrics on message buffering
            if let Some(state_manager) = &self.state_manager {
                state_manager.do_send(UpdateClientMessageMetrics {
                    client_id: self.client_id,
                    sent: false, // Not sent yet, just buffered
                    bytes: None, // No byte count for just buffering
                });
            }
            
            message_id
        }
    }

    // Send buffered messages with optional batching to avoid flooding
    fn send_buffered_messages(&mut self, ctx: &mut ws::WebsocketContext<Self>) {
        if self.message_buffer.is_empty() {
            return;
        }
        
        tracing::info!(
            "Sending {} buffered messages for client: {}", 
            self.message_buffer.len(), self.client_id
        );
        
        // Take at most 10 messages at a time to avoid flooding
        let batch_size = std::cmp::min(10, self.message_buffer.len());
        for _ in 0..batch_size {
            if let Some(msg) = self.message_buffer.pop_front() {
                ctx.text(msg.clone()); // Fixed: Clone the message
                
                // Update metrics
                if let Some(state_manager) = &self.state_manager {
                    state_manager.do_send(UpdateClientMessageMetrics {
                        client_id: self.client_id,
                        sent: true,
                        bytes: Some(msg.len()),
                    });
                }
            }
        }
        
        // If more messages remain, schedule another send after a short delay
        if !self.message_buffer.is_empty() {
            let remaining = self.message_buffer.len();
            tracing::debug!("Scheduled sending of remaining {} messages", remaining);
            
            // Schedule next batch after a short delay
            ctx.run_later(Duration::from_millis(100), |act, ctx| {
                act.send_buffered_messages(ctx);
            });
        }
    }

    // Restore session state from StateManagerActor
    fn restore_session(&mut self, ctx: &mut ws::WebsocketContext<Self>) {
        if let Some(state_manager) = &self.state_manager {
            let client_id = self.client_id;
            let addr = ctx.address();
            
            tracing::info!("Requesting session state for client {}", client_id);
            
            // Create future to get session state
            let future = state_manager.send(GetSessionState { client_id });
            
            // Handle the future response properly
            actix::fut::wrap_future::<_, Self>(async move {
                match future.await {
                    Ok(Some(session)) => {
                        tracing::info!("Retrieved session state for client {}", client_id);
                        addr.do_send(session);
                    },
                    Ok(None) => {
                        tracing::debug!("No saved session state for client {}", client_id);
                    },
                    Err(e) => {
                        tracing::error!("Error retrieving session state: {}", e);
                    }
                }
            })
            .wait(ctx); // Fixed: wait method now available from imported trait
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
    
    // Save current session state
    fn save_session_state(&self) {
        if let Some(state_manager) = &self.state_manager {
            // Convert VecDeque to Vec for serialization
            let buffer_vec: Vec<String> = self.message_buffer.iter().cloned().collect();
            
            let session_state = SessionState {
                client_id: self.client_id,
                authenticated: self.authenticated,
                wallet_address: self.wallet_address.clone(),
                message_buffer: buffer_vec,
                last_seen: self.last_heartbeat,
                session_data: self.session_data.clone(),
            };
            
            state_manager.do_send(SaveSessionState { state: session_state });
            tracing::debug!("Saved session state for client {}", self.client_id);
        }
    }
    
    // Check for expired message acknowledgements and resend
    fn check_and_resend_pending_messages(&self, ctx: &mut ws::WebsocketContext<Self>) {
        // Only proceed if delivery confirmation is enabled
        if !self.delivery_confirmation {
            return;
        }
        
        // Check for expired messages
        let expired_ids = self.message_tracker.check_expired();
        if !expired_ids.is_empty() {
            tracing::warn!(
                "Client {} has {} unacknowledged messages, resending", 
                self.client_id, expired_ids.len()
            );
            
            // For each expired message, resend
            for msg_id in expired_ids {
                if let Some((content, _)) = self.message_tracker.pending_acks.get(&msg_id) {
                    tracing::debug!("Resending message {} to client {}", msg_id, self.client_id);
                    ctx.text(content.clone());
                    
                    // Update metrics
                    if let Some(state_manager) = &self.state_manager {
                        state_manager.do_send(UpdateClientMessageMetrics {
                            client_id: self.client_id,
                            sent: true,
                            bytes: Some(content.len()),
                        });
                    }
                }
            }
        }
    }
    
    // Process message acknowledgement
    fn process_ack(&mut self, msg_id: u64) {
        if self.message_tracker.confirm_delivery(msg_id) {
            tracing::debug!("Message {} acknowledged by client {}", msg_id, self.client_id);
        } else {
            tracing::warn!("Received ack for unknown message ID {} from client {}", 
                        msg_id, self.client_id);
        }
    }
    
    // Create acknowledgement message
    fn create_ack(&self, msg_id: u64, status: AckStatus) -> MessageAcknowledgement {
        MessageAcknowledgement {
            source_id: self.client_id.to_string(),
            message_id: msg_id,
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            status,
        }
    }
    
    // Handle client text messages
    fn handle_client_message(&mut self, text: String, ctx: &mut ws::WebsocketContext<Self>) {
        // Update metrics
        if let Some(state_manager) = &self.state_manager {
            state_manager.do_send(UpdateClientMessageMetrics {
                client_id: self.client_id,
                sent: false, // We're receiving this
                bytes: Some(text.len()),
            });
        }
        
        // Check if this is an acknowledgement message
        if text.contains("\"ack\":") || text.contains("\"message_id\":") {
            // Simple check - in production use proper JSON parsing
            if let Some(msg_id_start) = text.find("\"message_id\":") {
                let after_id = &text[msg_id_start + 13..]; // Skip "message_id":
                if let Some(end) = after_id.find(',').or_else(|| after_id.find('}')) {
                    if let Ok(msg_id) = after_id[..end].trim().parse::<u64>() {
                        self.process_ack(msg_id);
                        
                        // If this is just an ack message, don't forward to router
                        if text.contains("\"type\":\"ack\"") {
                            return;
                        }
                    }
                }
            }
        }
        
        // Create client message for router
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
            
        let client_msg = ClientMessage {
            client_id: self.client_id,
            content: text.clone(), // Fixed: Clone here to avoid move
            authenticated: self.authenticated,
            wallet_address: self.wallet_address.clone(),
            timestamp,
            message_id: None,
            requires_ack: false,
            session_id: self.session_id.clone(),
        };
        
        // Forward to router
        if let Some(router) = &self.router {
            match router.try_send(client_msg) {
                Ok(_) => {
                    tracing::debug!("Message forwarded to router for client {}", self.client_id);
                },
                Err(e) => {
                    tracing::error!("Failed to send message to router: {}", e);
                    ctx.text(r#"{"error":"Internal routing error"}"#);
                }
            }
        } else {
            tracing::error!("Router not available for client {}", self.client_id);
            ctx.text(r#"{"error":"Router not configured"}"#);
        }
    }
}

impl Actor for ClientSessionActor {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!("Client connected: {}", self.client_id);
        self.last_heartbeat = Instant::now();
        self.reconnect_attempts = 0; // Reset on successful connection
        self.is_connected = true; // Set connection status to true
        
        // Start heartbeat
        self.heartbeat(ctx);
        
        // Restore session state
        self.restore_session(ctx);
        
        // Notify state manager about connection
        if let Some(state_manager) = &self.state_manager {
            state_manager.do_send(UpdateClientState {
                client_id: self.client_id,
                state: ConnectionState::Connected,
                last_seen_update: true,
            });
        }
        
        // Log session creation
        if let Some(session_id) = &self.session_id {
            tracing::info!("Session started for client {}: {}", self.client_id, session_id);
            
            // Optionally notify router about new session
            if let Some(router) = &self.router {
                let system_msg = SystemMessage::SessionCreated {
                    client_id: self.client_id,
                    session_id: session_id.clone(),
                };
                
                router.do_send(system_msg);
            }
        }
        
        // Send any existing buffered messages (if any)
        if !self.message_buffer.is_empty() {
            ctx.run_later(Duration::from_millis(100), |act, ctx| {
                act.send_buffered_messages(ctx);
            });
        }
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        tracing::info!("Client disconnected: {}", self.client_id);
        self.is_connected = false; // Set connection status to false
        
        // Save session state before stopping
        self.save_session_state();
        
        // Update state manager
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
        
        // Also unregister from router
        if let Some(router) = &self.router {
            router.do_send(super::router_actor::UnregisterClient {
                client_id: self.client_id,
            });
            
            // Notify about session end if needed
            if let Some(session_id) = &self.session_id {
                router.do_send(SystemMessage::SessionExpired {
                    client_id: self.client_id,
                    session_id: session_id.clone(),
                });
            }
        }
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for ClientSessionActor {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(msg)) => {
                self.last_heartbeat = Instant::now();
                self.update_activity(false);
                ctx.pong(&msg);
                
                // Reset reconnection attempts on successful ping
                if self.reconnect_attempts > 0 {
                    tracing::info!("Client {} reconnected successfully via ping", self.client_id);
                    self.reconnect_attempts = 0;
                }
            },
            Ok(ws::Message::Pong(_)) => {
                self.last_heartbeat = Instant::now();
                self.update_activity(false);
                
                // Reset reconnection attempts on successful pong
                if self.reconnect_attempts > 0 {
                    tracing::info!("Client {} reconnected successfully via pong", self.client_id);
                    self.reconnect_attempts = 0;
                }
            },
            Ok(ws::Message::Text(text)) => {
                self.last_heartbeat = Instant::now();
                self.update_activity(true);
                tracing::debug!("Received text message from client {}: {} bytes", 
                              self.client_id, text.len());
                
                // Use our enhanced message handler
                self.handle_client_message(text.to_string(), ctx);
            },
            Ok(ws::Message::Binary(bin)) => {
                self.last_heartbeat = Instant::now();
                self.update_activity(true);
                tracing::warn!("Binary messages not supported for client: {}", self.client_id);
                ctx.text(r#"{"error":"Binary messages not supported"}"#);
                
                // Update metrics for binary messages too
                if let Some(state_manager) = &self.state_manager {
                    state_manager.do_send(UpdateClientMessageMetrics {
                        client_id: self.client_id,
                        sent: false, // Received, not sent
                        bytes: Some(bin.len()),
                    });
                }
            },
            Ok(ws::Message::Close(reason)) => {
                tracing::info!("Client closing connection: {:?}", reason);
                
                // Save session state before closing
                self.save_session_state();
                
                if let Some(state_manager) = &self.state_manager {
                     state_manager.do_send(UpdateClientState {
                         client_id: self.client_id,
                         state: ConnectionState::Disconnected,
                         last_seen_update: true,
                     });
                }
                
                ctx.close(reason); // Will trigger stopped()
            },
            Ok(ws::Message::Continuation(_)) => {
                self.last_heartbeat = Instant::now();
                tracing::trace!("Received continuation frame from client {}", self.client_id);
            },
            Ok(ws::Message::Nop) => {
                self.last_heartbeat = Instant::now();
                tracing::trace!("Received nop frame from client {}", self.client_id);
            },
            Err(e) => {
                tracing::error!("WebSocket protocol error from client {}: {}", self.client_id, e);
                
                // Save session state on error
                self.save_session_state();
                
                if let Some(state_manager) = &self.state_manager {
                     state_manager.do_send(UpdateClientState {
                         client_id: self.client_id,
                         state: ConnectionState::Error,
                         last_seen_update: true,
                     });
                }
                
                ctx.stop();
            }
        }
    }
}

// Handle messages FROM the router TO this client
impl Handler<ClientActorMessage> for ClientSessionActor {
    type Result = ();

    fn handle(&mut self, msg: ClientActorMessage, ctx: &mut Self::Context) -> Self::Result {
        let content = msg.content.clone();
        tracing::info!("Received message via router for client {}, {} bytes", 
                      self.client_id, content.len());
        
        // Update metrics for sending to client
        if let Some(state_manager) = &self.state_manager {
            state_manager.do_send(UpdateClientMessageMetrics {
                client_id: self.client_id,
                sent: true,
                bytes: Some(content.len()),
            });
        }
        
        // Check if WebSocket is connected
        if !self.is_connected { // Fixed: Use is_connected field instead of ctx.connected()
            tracing::warn!("Client {} WebSocket not connected, buffering message", self.client_id);
            self.buffer_message(content);
            return;
        }
        
        // Check if we should add message ID for delivery confirmation
        if self.delivery_confirmation {
            // Try to parse as JSON to add message ID
            // For real implementation, use proper JSON parsing libraries
            if content.trim_start().starts_with('{') && content.trim_end().ends_with('}') {
                let msg_id = self.message_tracker.next_id();
                
                // Add message ID to content
                let content_with_id = if content.contains("\"message_id\":") {
                    // Already has message ID
                    content
                } else {
                    // Add message ID
                    let content_without_brace = content.trim_end_matches('}');
                    if content_without_brace.ends_with(',') {
                        format!("{}\"message_id\":{}}}", content_without_brace, msg_id)
                    } else {
                        format!("{},\"message_id\":{}}}", content_without_brace, msg_id)
                    }
                };
                
                // Track message for delivery confirmation
                self.message_tracker.add_pending(msg_id, content_with_id.clone());
                
                // Send to client
                ctx.text(content_with_id);
                tracing::debug!(
                    "Sent message to client {} with tracking ID {}", 
                    self.client_id, msg_id
                );
            } else {
                // Not valid JSON, send as-is without tracking
                ctx.text(content);
                tracing::debug!("Sent untracked message to client {}", self.client_id);
            }
        } else {
            // No delivery confirmation, send as-is
            ctx.text(content);
        }
    }
}

// Handler for SessionState to restore session
impl Handler<SessionState> for ClientSessionActor {
    type Result = ();
    
    fn handle(&mut self, msg: SessionState, ctx: &mut Self::Context) -> Self::Result {
        if msg.client_id == self.client_id {
            tracing::info!("Restoring session state for client {}", self.client_id);
            
            // Restore authentication state
            self.authenticated = msg.authenticated;
            self.wallet_address = msg.wallet_address;
            
            // Queue messages from saved session
            for message in msg.message_buffer {
                self.message_buffer.push_back(message);
            }
            
            // Restore session data
            self.session_data = msg.session_data;
            
            // Send buffered messages
            if !self.message_buffer.is_empty() {
                self.send_buffered_messages(ctx);
            }
            
            // Notify about session restoration
            if let Some(session_id) = &self.session_id {
                if let Some(router) = &self.router {
                    router.do_send(SystemMessage::SessionRestored {
                        client_id: self.client_id,
                        session_id: session_id.clone(),
                    });
                }
            }
            
            tracing::info!("Session restored for client {}", self.client_id);
        }
    }
}

// Handler for message acknowledgements
impl Handler<MessageAcknowledgement> for ClientSessionActor {
    type Result = ();
    
    fn handle(&mut self, msg: MessageAcknowledgement, _ctx: &mut Self::Context) -> Self::Result {
        tracing::debug!("Received acknowledgement for message {}: {:?}", msg.message_id, msg.status);
        
        // Process acknowledgement
        self.process_ack(msg.message_id);
    }
}