// websocket-server/src/actors/state_manager.rs

use actix::{Actor, Context, Handler, Message, Addr, AsyncContext};
use dashmap::DashMap;
use uuid::Uuid;
use std::time::{Duration, Instant};
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

// Enhanced client data structure
pub struct ClientData {
    pub addr: Addr<ClientSessionActor>,
    pub state: ConnectionState,
    pub last_seen: Instant,
    pub connected_at: Instant,
    pub authenticated: bool,
    pub wallet_address: Option<String>,
    pub reconnect_attempts: u32,
    pub last_message_at: Option<Instant>,
}

// Enhanced agent data structure
pub struct AgentData {
    pub addr: Addr<AgentActor>,
    pub state: ConnectionState,
    pub last_seen: Instant,
    pub connected_at: Instant,
    pub reconnect_attempts: u32,
    pub last_message_at: Option<Instant>,
}

// Message types
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

// New message for updating client state
#[derive(Message)]
#[rtype(result = "()")]
pub struct UpdateClientState {
    pub client_id: Uuid,
    pub state: ConnectionState,
    pub last_seen_update: bool,
}

// New message for updating agent state
#[derive(Message)]
#[rtype(result = "()")]
pub struct UpdateAgentState {
    pub agent_id: String,
    pub state: ConnectionState,
    pub last_seen_update: bool,
}

// Message for client activity update (heartbeat or message)
#[derive(Message)]
#[rtype(result = "()")]
pub struct ClientActivity {
    pub client_id: Uuid,
    pub is_message: bool,
}

// Message for agent activity update (heartbeat or message)
#[derive(Message)]
#[rtype(result = "()")]
pub struct AgentActivity {
    pub agent_id: String,
    pub is_message: bool,
}

// Request message to get client status
#[derive(Message)]
#[rtype(result = "Option<ClientStatusResponse>")]
pub struct GetClientStatus {
    pub client_id: Uuid,
}

// Response with client status
#[derive(Debug)]
pub struct ClientStatusResponse {
    pub client_id: Uuid,
    pub state: ConnectionState,
    pub connected_duration: Duration,
    pub last_seen_ago: Duration,
    pub authenticated: bool,
    pub reconnect_attempts: u32,
}

// Request message to get agent status
#[derive(Message)]
#[rtype(result = "Option<AgentStatusResponse>")]
pub struct GetAgentStatus {
    pub agent_id: String,
}

// Response with agent status
#[derive(Debug)]
pub struct AgentStatusResponse {
    pub agent_id: String,
    pub state: ConnectionState,
    pub connected_duration: Duration,
    pub last_seen_ago: Duration,
    pub reconnect_attempts: u32,
}

// Message to set router reference
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
    // Configuration
    client_timeout: Duration,
    agent_timeout: Duration,
    cleanup_interval: Duration,
    max_reconnect_attempts: u32,
}

impl StateManagerActor {
    pub fn new() -> Self {
        Self {
            clients: DashMap::new(),
            agents: DashMap::new(),
            router: None,
            // Default configuration
            client_timeout: Duration::from_secs(60),   // 1 minute timeout
            agent_timeout: Duration::from_secs(120),   // 2 minutes timeout
            cleanup_interval: Duration::from_secs(30), // Check every 30 seconds
            max_reconnect_attempts: 10,                // Max reconnection attempts
        }
    }
    
    // Set router reference
    pub fn set_router(&mut self, router_addr: Addr<RouterActor>) {
        self.router = Some(router_addr);
    }
    
    // Initialize monitoring tasks
    fn start_monitoring_tasks(&self, ctx: &mut Context<Self>) {
        // Connection monitoring task
        ctx.run_interval(self.cleanup_interval, |act, _ctx| {
            act.monitor_connections();
        });
    }
    
    // Monitor connections for timeouts and cleanup
    fn monitor_connections(&self) {
        let now = Instant::now();
        
        // Monitor client connections
        for entry in self.clients.iter() {
            let client_id = *entry.key();
            let client_data = entry.value();
            
            // Check for timeout based on state
            match client_data.state {
                ConnectionState::Connected => {
                    if now.duration_since(client_data.last_seen) > self.client_timeout {
                        tracing::warn!("Client timeout detected: {}", client_id);
                        
                        if let Some(mut client) = self.clients.get_mut(&client_id) {
                            // Update state to disconnected
                            client.state = ConnectionState::Disconnected;
                            
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
                        tracing::info!("Removing stale client: {}", client_id);
                        self.clients.remove(&client_id);
                    }
                },
                _ => {}
            }
        }
        
        // Monitor agent connections
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
        
        // Log connection statistics
        tracing::debug!(
            "Connection statistics - Clients: {}, Agents: {}", 
            self.clients.len(), 
            self.agents.len()
        );
    }
}

impl Actor for StateManagerActor {
    type Context = Context<Self>;
    
    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!("StateManagerActor started");
        
        // Start monitoring tasks
        self.start_monitoring_tasks(ctx);
    }
    
    fn stopped(&mut self, _ctx: &mut Self::Context) {
        tracing::info!("StateManagerActor stopped");
    }
}

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
            // Create new client entry
            let client_data = ClientData {
                addr: msg.addr.clone(),
                state: ConnectionState::Connected,
                last_seen: now,
                connected_at: now,
                authenticated: msg.authenticated,
                wallet_address: msg.wallet_address.clone(),
                reconnect_attempts: 0,
                last_message_at: None,
            };
            
            self.clients.insert(msg.client_id, client_data);
            tracing::info!("Client registered: {}", msg.client_id);
        }
        
        // Notify router about client connection
        if let Some(router) = &self.router {
            router.do_send(SystemMessage::ClientConnected {
                client_id: msg.client_id,
                authenticated: msg.authenticated,
                wallet_address: msg.wallet_address,
            });
            
            // Register with router
            router.do_send(super::router_actor::RegisterClient {
                client_id: msg.client_id,
                addr: msg.addr,
            });
        }
    }
}

impl Handler<UnregisterClient> for StateManagerActor {
    type Result = ();
    
    fn handle(&mut self, msg: UnregisterClient, _ctx: &mut Self::Context) -> Self::Result {
        // Mark client as disconnected but keep in map for potential reconnection
        if let Some(mut entry) = self.clients.get_mut(&msg.client_id) {
            entry.state = ConnectionState::Disconnected;
            entry.last_seen = Instant::now();
            
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
            // Create new agent entry
            let agent_data = AgentData {
                addr: msg.addr.clone(),
                state: ConnectionState::Connected,
                last_seen: now,
                connected_at: now,
                reconnect_attempts: 0,
                last_message_at: None,
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

impl Handler<UnregisterAgent> for StateManagerActor {
    type Result = ();
    
    fn handle(&mut self, msg: UnregisterAgent, _ctx: &mut Self::Context) -> Self::Result {
        // Mark agent as disconnected but keep in map for potential reconnection
        if let Some(mut entry) = self.agents.get_mut(&msg.agent_id) {
            entry.state = ConnectionState::Disconnected;
            entry.last_seen = Instant::now();
            
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