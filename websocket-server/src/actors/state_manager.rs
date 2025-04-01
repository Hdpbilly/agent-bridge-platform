// websocket-server/src/actors/state_manager.rs

use actix::{Actor, Context, Handler, Message, Addr, AsyncContext};
use dashmap::DashMap;
use uuid::Uuid;
use std::time::{Duration, Instant};
use std::collections::HashMap;
use chrono::{DateTime, Utc};
use super::client_session_actor::ClientSessionActor;
use super::agent_actor::AgentActor;
use super::router_actor::RouterActor;
use common::SystemMessage;

// Enhanced connection states
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConnectionState {
    Connected,
    Disconnected,
    Reconnecting,
    Idle,
    Error,
}

// New: Session state structure for persistence
#[derive(Debug, Clone, Message)]
#[rtype(result = "()")]
pub struct SessionState {
    pub client_id: Uuid,
    pub authenticated: bool,
    pub wallet_address: Option<String>,
    pub message_buffer: Vec<String>,
    pub last_seen: Instant,
    pub session_data: HashMap<String, String>,
}

// New: Message to save session state
#[derive(Message)]
#[rtype(result = "()")]
pub struct SaveSessionState {
    pub state: SessionState,
}

// New: Message to get session state
#[derive(Message)]
#[rtype(result = "Option<SessionState>")]
pub struct GetSessionState {
    pub client_id: Uuid,
}

// Enhanced client data structure with metrics
pub struct ClientData {
    pub addr: Addr<ClientSessionActor>,
    pub state: ConnectionState,
    pub last_seen: Instant,
    pub connected_at: Instant,
    pub authenticated: bool,
    pub wallet_address: Option<String>,
    pub reconnect_attempts: u32,
    pub last_message_at: Option<Instant>,
    // New metrics fields - initialized with defaults
    pub message_count_sent: u64,
    pub message_count_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub disconnection_count: u32,
}

// Enhanced agent data structure with metrics
pub struct AgentData {
    pub addr: Addr<AgentActor>,
    pub state: ConnectionState,
    pub last_seen: Instant,
    pub connected_at: Instant,
    pub reconnect_attempts: u32,
    pub last_message_at: Option<Instant>,
    // New metrics fields - initialized with defaults
    pub message_count_sent: u64,
    pub message_count_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub disconnection_count: u32,
}

// Existing message types (unchanged)
#[derive(Message)]
#[rtype(result = "()")]
pub struct RegisterClient {
    pub client_id: Uuid,
    pub addr: Addr<ClientSessionActor>,
    pub authenticated: bool,
    pub wallet_address: Option<String>,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct UnregisterClient {
    pub client_id: Uuid,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct RegisterAgent {
    pub agent_id: String,
    pub addr: Addr<AgentActor>,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct UnregisterAgent {
    pub agent_id: String,
}

// Existing messages (unchanged)
#[derive(Message)]
#[rtype(result = "()")]
pub struct UpdateClientState {
    pub client_id: Uuid,
    pub state: ConnectionState,
    pub last_seen_update: bool,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct UpdateAgentState {
    pub agent_id: String,
    pub state: ConnectionState,
    pub last_seen_update: bool,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct ClientActivity {
    pub client_id: Uuid,
    pub is_message: bool,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct AgentActivity {
    pub agent_id: String,
    pub is_message: bool,
}

// Enhanced response with client status including metrics
#[derive(Message)]
#[rtype(result = "Option<ClientStatusResponse>")]
pub struct GetClientStatus {
    pub client_id: Uuid,
}

#[derive(Debug)]
pub struct ClientStatusResponse {
    pub client_id: Uuid,
    pub state: ConnectionState,
    pub connected_duration: Duration,
    pub last_seen_ago: Duration,
    pub authenticated: bool,
    pub reconnect_attempts: u32,
    // New metrics fields
    pub message_count_sent: u64,
    pub message_count_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub disconnection_count: u32,
}

// Enhanced response with agent status including metrics
#[derive(Message)]
#[rtype(result = "Option<AgentStatusResponse>")]
pub struct GetAgentStatus {
    pub agent_id: String,
}

#[derive(Debug)]
pub struct AgentStatusResponse {
    pub agent_id: String,
    pub state: ConnectionState,
    pub connected_duration: Duration,
    pub last_seen_ago: Duration,
    pub reconnect_attempts: u32,
    // New metrics fields
    pub message_count_sent: u64,
    pub message_count_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub disconnection_count: u32,
}

// New: Message to fetch system metrics
#[derive(Message)]
#[rtype(result = "SystemMetrics")]
pub struct GetSystemMetrics;

// New: Response with system-wide metrics
#[derive(Debug)]
pub struct SystemMetrics {
    pub total_clients: usize,
    pub active_clients: usize,
    pub total_agents: usize,
    pub active_agents: usize,
    pub total_messages_processed: u64,
    pub messages_per_second: f64,
    pub bytes_transferred: u64,
    pub timestamp: std::time::SystemTime, // Changed from DateTime<Utc>
}

// New: Message to update message metrics
#[derive(Message)]
#[rtype(result = "()")]
pub struct UpdateClientMessageMetrics {
    pub client_id: Uuid,
    pub sent: bool,
    pub bytes: Option<usize>,
}

// New: Message to update agent message metrics
#[derive(Message)]
#[rtype(result = "()")]
pub struct UpdateAgentMessageMetrics {
    pub agent_id: String,
    pub sent: bool,
    pub bytes: Option<usize>,
}

// Unchanged
#[derive(Message)]
#[rtype(result = "()")]
pub struct SetRouter {
    pub router: Addr<RouterActor>,
}

// Enhanced state manager actor
pub struct StateManagerActor {
    clients: DashMap<Uuid, ClientData>,
    agents: DashMap<String, AgentData>,
    router: Option<Addr<RouterActor>>,
    // New fields for session persistence and metrics
    sessions: DashMap<Uuid, SessionState>,
    total_messages: u64,
    last_metrics_update: Instant,
    message_rate_window: Vec<(Instant, u64)>,
    bytes_transferred: u64,
    // Configuration
    client_timeout: Duration,
    agent_timeout: Duration,
    cleanup_interval: Duration,
    metrics_interval: Duration,
    max_reconnect_attempts: u32,
    session_ttl: Duration,
}

impl StateManagerActor {
    pub fn new() -> Self {
        Self {
            clients: DashMap::new(),
            agents: DashMap::new(),
            router: None,
            // Initialize new fields
            sessions: DashMap::new(),
            total_messages: 0,
            last_metrics_update: Instant::now(),
            message_rate_window: Vec::new(),
            bytes_transferred: 0,
            // Default configuration - unchanged
            client_timeout: Duration::from_secs(60),   // 1 minute timeout
            agent_timeout: Duration::from_secs(120),   // 2 minutes timeout
            cleanup_interval: Duration::from_secs(30), // Check every 30 seconds
            // New configuration values
            metrics_interval: Duration::from_secs(5),   // Update metrics every 5 seconds
            max_reconnect_attempts: 10,                // Max reconnection attempts
            session_ttl: Duration::from_secs(3600),    // 1 hour session TTL
        }
    }
    
    // Unchanged
    pub fn set_router(&mut self, router_addr: Addr<RouterActor>) {
        self.router = Some(router_addr);
    }
    
    // Enhanced to also start metrics and session cleanup tasks
    fn start_monitoring_tasks(&self, ctx: &mut Context<Self>) {
        // Existing connection monitoring task
        ctx.run_interval(self.cleanup_interval, |act, _ctx| {
            act.monitor_connections();
        });
        
        // New metrics collection task
        ctx.run_interval(self.metrics_interval, |act, _ctx| {
            act.update_metrics();
        });
        
        // New session cleanup task
        ctx.run_interval(Duration::from_secs(300), |act, _ctx| { // Run every 5 minutes
            act.cleanup_expired_sessions();
        });
    }
    
    // Enhanced connection monitoring with session saving
    fn monitor_connections(&self) {
        let now = Instant::now();
        
        // Monitor client connections (similar logic but added session saving)
        for entry in self.clients.iter() {
            let client_id = *entry.key();
            let client_data = entry.value();
            
            // Check for timeout based on state
            match client_data.state {
                ConnectionState::Connected => {
                    if now.duration_since(client_data.last_seen) > self.client_timeout {
                        tracing::warn!("Client timeout detected: {}", client_id);
                        
                        // Save minimal session state before marking as disconnected
                        let session_state = SessionState {
                            client_id,
                            authenticated: client_data.authenticated,
                            wallet_address: client_data.wallet_address.clone(),
                            message_buffer: Vec::new(), // Can't access client message buffer from here
                            last_seen: client_data.last_seen,
                            session_data: HashMap::new(), // Initialize empty
                        };
                        self.sessions.insert(client_id, session_state);
                        
                        if let Some(mut client) = self.clients.get_mut(&client_id) {
                            // Update state to disconnected
                            client.state = ConnectionState::Disconnected;
                            client.disconnection_count += 1; // Update metrics
                            
                            // Notify router about disconnection
                            if let Some(router) = &self.router {
                                router.do_send(SystemMessage::ClientDisconnected { 
                                    client_id 
                                });
                            }
                        }
                    }
                },
                ConnectionState::Reconnecting => {
                    // Check if exceeded max reconnect attempts
                    if client_data.reconnect_attempts >= self.max_reconnect_attempts {
                        tracing::warn!("Client exceeded max reconnect attempts: {}", client_id);
                        
                        if let Some(mut client) = self.clients.get_mut(&client_id) {
                            // Update state to error
                            client.state = ConnectionState::Error;
                        }
                    }
                },
                ConnectionState::Disconnected | ConnectionState::Error => {
                    // Check if disconnected for too long (3x timeout)
                    if now.duration_since(client_data.last_seen) > self.client_timeout.mul_f32(3.0) {
                        tracing::info!("Removing stale client from active tracking: {}", client_id);
                        // We don't remove session data yet - will be handled by session cleanup task
                        self.clients.remove(&client_id);
                    }
                },
                _ => {}
            }
        }
        
        // Monitor agent connections (similar logic to existing implementation)
        for entry in self.agents.iter() {
            let agent_id = entry.key().clone();
            let agent_data = entry.value();
            
            // Check for timeout based on state
            match agent_data.state {
                ConnectionState::Connected => {
                    if now.duration_since(agent_data.last_seen) > self.agent_timeout {
                        tracing::warn!("Agent timeout detected: {}", agent_id);
                        
                        if let Some(mut agent) = self.agents.get_mut(&agent_id) {
                            // Update state to disconnected
                            agent.state = ConnectionState::Disconnected;
                            agent.disconnection_count += 1; // Update metrics
                            
                            // Notify router about disconnection
                            if let Some(router) = &self.router {
                                router.do_send(SystemMessage::AgentDisconnected);
                            }
                        }
                    }
                },
                ConnectionState::Reconnecting => {
                    // Check if exceeded max reconnect attempts
                    if agent_data.reconnect_attempts >= self.max_reconnect_attempts {
                        tracing::warn!("Agent exceeded max reconnect attempts: {}", agent_id);
                        
                        if let Some(mut agent) = self.agents.get_mut(&agent_id) {
                            // Update state to error
                            agent.state = ConnectionState::Error;
                        }
                    }
                },
                ConnectionState::Disconnected | ConnectionState::Error => {
                    // Check if disconnected for too long (3x timeout)
                    if now.duration_since(agent_data.last_seen) > self.agent_timeout.mul_f32(3.0) {
                        tracing::info!("Removing stale agent: {}", agent_id);
                        self.agents.remove(&agent_id);
                    }
                },
                _ => {}
            }
        }
        
        // Log connection statistics with enhanced metrics
        let active_clients = self.clients.iter()
            .filter(|e| e.value().state == ConnectionState::Connected)
            .count();
        let active_agents = self.agents.iter()
            .filter(|e| e.value().state == ConnectionState::Connected)
            .count();
        
        tracing::debug!(
            "Connection statistics - Clients: {}/{} active, Agents: {}/{} active, Sessions: {}", 
            active_clients, self.clients.len(),
            active_agents, self.agents.len(),
            self.sessions.len()
        );
    }
    
    // New: Update system-wide metrics
    fn update_metrics(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_metrics_update);
        self.last_metrics_update = now;
        
        // Maintain metrics window (sliding window of the last 60 seconds)
        // Remove entries older than 60 seconds
        let mut updated_window = Vec::new();
        for &(timestamp, count) in &self.message_rate_window {
            if now.duration_since(timestamp) < Duration::from_secs(60) {
                updated_window.push((timestamp, count));
            }
        }
        
        // Add current entry
        updated_window.push((now, self.total_messages));
        self.message_rate_window = updated_window;
        
        // Calculate messages per second
        let messages_per_second = if self.message_rate_window.len() > 1 {
            let (oldest_time, oldest_count) = self.message_rate_window[0];
            let (newest_time, newest_count) = self.message_rate_window[self.message_rate_window.len() - 1];
            
            let time_diff = newest_time.duration_since(oldest_time).as_secs_f64();
            let message_diff = newest_count - oldest_count;
            
            if time_diff > 0.0 {
                message_diff as f64 / time_diff
            } else {
                0.0
            }
        } else {
            0.0
        };
        
        // Log metrics summary
        tracing::info!(
            "System metrics - Clients: {}, Agents: {}, Messages: {}, Rate: {:.2} msg/s, Bandwidth: {} bytes",
            self.clients.len(),
            self.agents.len(),
            self.total_messages,
            messages_per_second,
            self.bytes_transferred
        );
    }
    
    // New: Clean up expired sessions
    fn cleanup_expired_sessions(&self) {
        let now = Instant::now();
        let mut expired_count = 0;
        
        // Collect expired session IDs
        let expired_sessions = self.sessions.iter()
            .filter(|entry| now.duration_since(entry.value().last_seen) > self.session_ttl)
            .map(|entry| *entry.key())
            .collect::<Vec<_>>();
        
        // Remove expired sessions
        for client_id in expired_sessions {
            tracing::info!("Removing expired session for client: {}", client_id);
            self.sessions.remove(&client_id);
            expired_count += 1;
        }
        
        if expired_count > 0 {
            tracing::info!("Cleaned up {} expired sessions, remaining: {}", 
                         expired_count, self.sessions.len());
        }
    }
}

impl Actor for StateManagerActor {
    type Context = Context<Self>;
    
    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!("StateManagerActor started with session persistence and metrics");
        
        // Start monitoring tasks (including new ones)
        self.start_monitoring_tasks(ctx);
        
        // Log initial configuration
        tracing::info!(
            "StateManagerActor config - Session TTL: {}s, Client timeout: {}s, Agent timeout: {}s",
            self.session_ttl.as_secs(),
            self.client_timeout.as_secs(),
            self.agent_timeout.as_secs()
        );
    }
    
    fn stopped(&mut self, _ctx: &mut Self::Context) {
        tracing::info!(
            "StateManagerActor stopped - Final metrics: Clients: {}, Agents: {}, Messages: {}, Bandwidth: {} bytes",
            self.clients.len(),
            self.agents.len(),
            self.total_messages,
            self.bytes_transferred
        );
    }
}

// Implement message handlers with backward compatibility
// and enhancements for sessions and metrics

impl Handler<RegisterClient> for StateManagerActor {
    type Result = ();
    
    fn handle(&mut self, msg: RegisterClient, _ctx: &mut Self::Context) -> Self::Result {
        let now = Instant::now();
        
        // Check if client already exists
        if let Some(mut entry) = self.clients.get_mut(&msg.client_id) {
            // Update existing client entry
            entry.addr = msg.addr.clone();
            entry.state = ConnectionState::Connected;
            entry.last_seen = now;
            entry.authenticated = msg.authenticated;
            entry.wallet_address = msg.wallet_address.clone();
            entry.reconnect_attempts = 0; // Reset reconnect attempts on successful reconnection
            
            tracing::info!("Client reconnected: {}", msg.client_id);
        } else {
            // Create new client entry with metrics initialized to zero
            let client_data = ClientData {
                addr: msg.addr.clone(),
                state: ConnectionState::Connected,
                last_seen: now,
                connected_at: now,
                authenticated: msg.authenticated,
                wallet_address: msg.wallet_address.clone(),
                reconnect_attempts: 0,
                last_message_at: None,
                // Initialize metrics
                message_count_sent: 0,
                message_count_received: 0,
                bytes_sent: 0,
                bytes_received: 0,
                disconnection_count: 0,
            };
            
            self.clients.insert(msg.client_id, client_data);
            tracing::info!("Client registered: {}", msg.client_id);
        }
        
        // New: Check for existing session state to restore
        if let Some(session) = self.sessions.get(&msg.client_id) {
            tracing::info!("Found existing session for client {}, will restore later", msg.client_id);
            
            // Instead of sending SessionState directly (which causes type mismatch),
            // we'll store the client actor address and retrieve the session when needed
            // The client actor will need to request its session state
            
            // For now, just keep the session in storage
            // Remove this line that causes the type error:
            // msg.addr.do_send(session.clone());
        }
        
        // Notify router about client connection
        if let Some(router) = &self.router {
            router.do_send(SystemMessage::ClientConnected {
                client_id: msg.client_id,
                authenticated: msg.authenticated,
                wallet_address: msg.wallet_address.clone(),
            });
            
            // Register with router
            router.do_send(super::router_actor::RegisterClient {
                client_id: msg.client_id,
                addr: msg.addr,
            });
        }
    }
}

impl Handler<RegisterAgent> for StateManagerActor {
    type Result = ();
    
    fn handle(&mut self, msg: RegisterAgent, _ctx: &mut Self::Context) -> Self::Result {
        let now = Instant::now();
        
        // Check if agent already exists
        if let Some(mut entry) = self.agents.get_mut(&msg.agent_id) {
            // Update existing agent entry
            entry.addr = msg.addr.clone();
            entry.state = ConnectionState::Connected;
            entry.last_seen = now;
            entry.reconnect_attempts = 0; // Reset reconnect attempts on successful reconnection
            
            tracing::info!("Agent reconnected: {}", msg.agent_id);
        } else {
            // Create new agent entry with metrics initialized to zero
            let agent_data = AgentData {
                addr: msg.addr.clone(),
                state: ConnectionState::Connected,
                last_seen: now,
                connected_at: now,
                reconnect_attempts: 0,
                last_message_at: None,
                // Initialize metrics
                message_count_sent: 0,
                message_count_received: 0,
                bytes_sent: 0,
                bytes_received: 0,
                disconnection_count: 0,
            };
            
            self.agents.insert(msg.agent_id.clone(), agent_data);
            tracing::info!("Agent registered: {}", msg.agent_id);
        }
        
        // Notify router about agent connection
        if let Some(router) = &self.router {
            router.do_send(SystemMessage::AgentConnected);
            
            // Register with router
            router.do_send(super::router_actor::RegisterAgent {
                agent_id: msg.agent_id,
                addr: msg.addr,
            });
        }
    }
}

impl Handler<UnregisterClient> for StateManagerActor {
    type Result = ();
    
    fn handle(&mut self, msg: UnregisterClient, _ctx: &mut Self::Context) -> Self::Result {
        // Try to save session state if client exists
        if let Some(client) = self.clients.get(&msg.client_id) {
            // Create minimal session state
            let session_state = SessionState {
                client_id: msg.client_id,
                authenticated: client.authenticated,
                wallet_address: client.wallet_address.clone(),
                message_buffer: Vec::new(), // Can't access client's buffer from here
                last_seen: client.last_seen,
                session_data: HashMap::new(), // Initialize empty
            };
            
            // Save session state
            self.sessions.insert(msg.client_id, session_state);
            tracing::debug!("Saved session state for unregistering client: {}", msg.client_id);
        }
        
        // Mark client as disconnected but keep in map for potential reconnection
        if let Some(mut entry) = self.clients.get_mut(&msg.client_id) {
            entry.state = ConnectionState::Disconnected;
            entry.last_seen = Instant::now();
            entry.disconnection_count += 1; // Update metrics
            
            tracing::info!("Client disconnected: {}", msg.client_id);
        }
        
        // Notify router about client disconnection
        if let Some(router) = &self.router {
            router.do_send(SystemMessage::ClientDisconnected {
                client_id: msg.client_id,
            });
            
            // Unregister from router
            router.do_send(super::router_actor::UnregisterClient {
                client_id: msg.client_id,
            });
        }
    }
}

impl Handler<UnregisterAgent> for StateManagerActor {
    type Result = ();
    
    fn handle(&mut self, msg: UnregisterAgent, _ctx: &mut Self::Context) -> Self::Result {
        // Mark agent as disconnected but keep in map for potential reconnection
        if let Some(mut entry) = self.agents.get_mut(&msg.agent_id) {
            entry.state = ConnectionState::Disconnected;
            entry.last_seen = Instant::now();
            entry.disconnection_count += 1; // Update metrics
            
            tracing::info!("Agent disconnected: {}", msg.agent_id);
        }
        
        // Notify router about agent disconnection
        if let Some(router) = &self.router {
            router.do_send(SystemMessage::AgentDisconnected);
            
            // Unregister from router
            router.do_send(super::router_actor::UnregisterAgent {
                agent_id: msg.agent_id,
            });
        }
    }
}

impl Handler<UpdateClientState> for StateManagerActor {
    type Result = ();
    
    fn handle(&mut self, msg: UpdateClientState, _ctx: &mut Self::Context) -> Self::Result {
        if let Some(mut entry) = self.clients.get_mut(&msg.client_id) {
            // Update state
            let old_state = entry.state;
            entry.state = msg.state;
            
            // Update last seen if requested
            if msg.last_seen_update {
                entry.last_seen = Instant::now();
            }
            
            // Log state change if different
            if old_state != msg.state {
                tracing::info!(
                    "Client {} state changed: {:?} -> {:?}", 
                    msg.client_id, old_state, msg.state
                );
                
                // Update reconnect attempts if transitioning to reconnecting
                if msg.state == ConnectionState::Reconnecting {
                    entry.reconnect_attempts += 1;
                    tracing::info!(
                        "Client {} reconnection attempt: {}/{}", 
                        msg.client_id, entry.reconnect_attempts, self.max_reconnect_attempts
                    );
                }
                
                // Update disconnection count if transitioning to disconnected
                if msg.state == ConnectionState::Disconnected {
                    entry.disconnection_count += 1;
                }
            }
        }
    }
}

impl Handler<UpdateAgentState> for StateManagerActor {
    type Result = ();
    
    fn handle(&mut self, msg: UpdateAgentState, _ctx: &mut Self::Context) -> Self::Result {
        if let Some(mut entry) = self.agents.get_mut(&msg.agent_id) {
            // Update state
            let old_state = entry.state;
            entry.state = msg.state;
            
            // Update last seen if requested
            if msg.last_seen_update {
                entry.last_seen = Instant::now();
            }
            
            // Log state change if different
            if old_state != msg.state {
                tracing::info!(
                    "Agent {} state changed: {:?} -> {:?}", 
                    msg.agent_id, old_state, msg.state
                );
                
                // Update reconnect attempts if transitioning to reconnecting
                if msg.state == ConnectionState::Reconnecting {
                    entry.reconnect_attempts += 1;
                    tracing::info!(
                        "Agent {} reconnection attempt: {}/{}", 
                        msg.agent_id, entry.reconnect_attempts, self.max_reconnect_attempts
                    );
                }
                
                // Update disconnection count if transitioning to disconnected
                if msg.state == ConnectionState::Disconnected {
                    entry.disconnection_count += 1;
                }
            }
        }
    }
}

impl Handler<ClientActivity> for StateManagerActor {
    type Result = ();
    
    fn handle(&mut self, msg: ClientActivity, _ctx: &mut Self::Context) -> Self::Result {
        if let Some(mut entry) = self.clients.get_mut(&msg.client_id) {
            // Update last seen
            entry.last_seen = Instant::now();
            
            // Update last message timestamp if this is a message activity
            if msg.is_message {
                entry.last_message_at = Some(Instant::now());
                
                // Update message count for metrics
                if msg.is_message {
                    entry.message_count_received += 1;
                    self.total_messages += 1;
                }
            }
            
            // If disconnected or reconnecting, update state to connected
            if entry.state == ConnectionState::Disconnected || 
               entry.state == ConnectionState::Reconnecting {
                entry.state = ConnectionState::Connected;
                entry.reconnect_attempts = 0;
                
                tracing::info!("Client {} reconnected through activity", msg.client_id);
                
                // Notify router about reconnection
                if let Some(router) = &self.router {
                    router.do_send(SystemMessage::ClientConnected {
                        client_id: msg.client_id,
                        authenticated: entry.authenticated,
                        wallet_address: entry.wallet_address.clone(),
                    });
                }
            }
        }
    }
}

impl Handler<AgentActivity> for StateManagerActor {
    type Result = ();
    
    fn handle(&mut self, msg: AgentActivity, _ctx: &mut Self::Context) -> Self::Result {
        if let Some(mut entry) = self.agents.get_mut(&msg.agent_id) {
            // Update last seen
            entry.last_seen = Instant::now();
            
            // Update last message timestamp if this is a message activity
            if msg.is_message {
                entry.last_message_at = Some(Instant::now());
                
                // Update message count for metrics
                if msg.is_message {
                    entry.message_count_received += 1;
                    self.total_messages += 1;
                }
            }
            
            // If disconnected or reconnecting, update state to connected
            if entry.state == ConnectionState::Disconnected || 
               entry.state == ConnectionState::Reconnecting {
                entry.state = ConnectionState::Connected;
                entry.reconnect_attempts = 0;
                
                tracing::info!("Agent {} reconnected through activity", msg.agent_id);
                
                // Notify router about reconnection
                if let Some(router) = &self.router {
                    router.do_send(SystemMessage::AgentConnected);
                }
            }
        }
    }
}

impl Handler<GetClientStatus> for StateManagerActor {
    type Result = Option<ClientStatusResponse>;
    
    fn handle(&mut self, msg: GetClientStatus, _ctx: &mut Self::Context) -> Self::Result {
        if let Some(entry) = self.clients.get(&msg.client_id) {
            let now = Instant::now();
            
            Some(ClientStatusResponse {
                client_id: msg.client_id,
                state: entry.state,
                connected_duration: now.duration_since(entry.connected_at),
                last_seen_ago: now.duration_since(entry.last_seen),
                authenticated: entry.authenticated,
                reconnect_attempts: entry.reconnect_attempts,
                // Include metrics in response
                message_count_sent: entry.message_count_sent,
                message_count_received: entry.message_count_received,
                bytes_sent: entry.bytes_sent,
                bytes_received: entry.bytes_received,
                disconnection_count: entry.disconnection_count,
            })
        } else {
            None
        }
    }
}

impl Handler<GetAgentStatus> for StateManagerActor {
    type Result = Option<AgentStatusResponse>;
    
    fn handle(&mut self, msg: GetAgentStatus, _ctx: &mut Self::Context) -> Self::Result {
        if let Some(entry) = self.agents.get(&msg.agent_id) {
            let now = Instant::now();
            
            Some(AgentStatusResponse {
                agent_id: msg.agent_id.clone(),
                state: entry.state,
                connected_duration: now.duration_since(entry.connected_at),
                last_seen_ago: now.duration_since(entry.last_seen),
                reconnect_attempts: entry.reconnect_attempts,
                // Include metrics in response
                message_count_sent: entry.message_count_sent,
                message_count_received: entry.message_count_received,
                bytes_sent: entry.bytes_sent,
                bytes_received: entry.bytes_received,
                disconnection_count: entry.disconnection_count,
            })
        } else {
            None
        }
    }
}

impl Handler<SetRouter> for StateManagerActor {
    type Result = ();
    
    fn handle(&mut self, msg: SetRouter, _ctx: &mut Self::Context) -> Self::Result {
        self.router = Some(msg.router);
    }
}

// New: Handle saving session state
impl Handler<SaveSessionState> for StateManagerActor {
    type Result = ();
    
    fn handle(&mut self, msg: SaveSessionState, _ctx: &mut Self::Context) -> Self::Result {
        tracing::info!("Saving session state for client: {}", msg.state.client_id);
        self.sessions.insert(msg.state.client_id, msg.state);
    }
}

// New: Handle getting session state
impl Handler<GetSessionState> for StateManagerActor {
    type Result = Option<SessionState>;
    
    fn handle(&mut self, msg: GetSessionState, _ctx: &mut Self::Context) -> Self::Result {
        if let Some(session) = self.sessions.get(&msg.client_id) {
            tracing::info!("Retrieved session state for client: {}", msg.client_id);
            Some(session.clone())
        } else {
            tracing::debug!("No session state found for client: {}", msg.client_id);
            None
        }
    }
}

// New: Handle system metrics query
impl Handler<GetSystemMetrics> for StateManagerActor {
    type Result = actix::MessageResult<GetSystemMetrics>;
    
    fn handle(&mut self, _msg: GetSystemMetrics, _ctx: &mut Self::Context) -> Self::Result {
        let active_clients = self.clients.iter()
            .filter(|entry| entry.value().state == ConnectionState::Connected)
            .count();
            
        let active_agents = self.agents.iter()
            .filter(|entry| entry.value().state == ConnectionState::Connected)
            .count();
            
        // Calculate message rate. Might need to keep tabs on this and optimize it and make sure that the handler is not easily accessed to any client. 
        let messages_per_second = if self.message_rate_window.len() > 1 {
            let (oldest_time, oldest_count) = self.message_rate_window[0];
            let (newest_time, newest_count) = self.message_rate_window[self.message_rate_window.len() - 1];
            
            let time_diff = newest_time.duration_since(oldest_time).as_secs_f64();
            let message_diff = newest_count - oldest_count;
            
            if time_diff > 0.0 {
                message_diff as f64 / time_diff
            } else {
                0.0
            }
        } else {
            0.0
        };
        
        let result = SystemMetrics {
            total_clients: self.clients.len(),
            active_clients,
            total_agents: self.agents.len(),
            active_agents,
            total_messages_processed: self.total_messages,
            messages_per_second,
            bytes_transferred: self.bytes_transferred,
            timestamp: std::time::SystemTime::now(),
        };
        actix::MessageResult(result)
    }
}

// New: Handle client message metrics update
impl Handler<UpdateClientMessageMetrics> for StateManagerActor {
    type Result = ();
    
    fn handle(&mut self, msg: UpdateClientMessageMetrics, _ctx: &mut Self::Context) -> Self::Result {
        if let Some(mut entry) = self.clients.get_mut(&msg.client_id) {
            if msg.sent {
                entry.message_count_sent += 1;
            } else {
                entry.message_count_received += 1;
            }
            
            // Update byte count if provided
            if let Some(bytes) = msg.bytes {
                if msg.sent {
                    entry.bytes_sent += bytes as u64;
                } else {
                    entry.bytes_received += bytes as u64;
                }
                
                // Update global bytes transferred
                self.bytes_transferred += bytes as u64;
            }
            
            // Update global message count
            self.total_messages += 1;
        }
    }
}

// New: Handle agent message metrics update
impl Handler<UpdateAgentMessageMetrics> for StateManagerActor {
    type Result = ();
    
    fn handle(&mut self, msg: UpdateAgentMessageMetrics, _ctx: &mut Self::Context) -> Self::Result {
        if let Some(mut entry) = self.agents.get_mut(&msg.agent_id) {
            if msg.sent {
                entry.message_count_sent += 1;
            } else {
                entry.message_count_received += 1;
            }
            
            // Update byte count if provided
            if let Some(bytes) = msg.bytes {
                if msg.sent {
                    entry.bytes_sent += bytes as u64;
                } else {
                    entry.bytes_received += bytes as u64;
                }
                
                // Update global bytes transferred
                self.bytes_transferred += bytes as u64;
            }
            
            // Update global message count
            self.total_messages += 1;
        }
    }
}