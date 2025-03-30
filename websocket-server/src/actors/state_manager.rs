// WebSocket Server - actors/state_manager.rs
// websocket-server/src/actors/state_manager.rs
use actix::{Actor, Context, Handler, Message, Addr, AsyncContext};
use dashmap::DashMap;
use uuid::Uuid;
use std::time::{Duration, Instant};
use super::client_session_actor::ClientSessionActor;
use super::agent_actor::AgentActor;
use super::router_actor::RouterActor;

// Enhanced messages for the state manager
#[derive(Message)]
#[rtype(result = "()")]
pub struct RegisterClient {
    pub client_id: Uuid,
    pub addr: Addr<ClientSessionActor>,
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

// New message type for client status updates
#[derive(Message)]
#[rtype(result = "()")]
pub struct ClientStatusUpdate {
    pub client_id: Uuid,
    pub connected: bool,
    pub last_seen: Instant,
}

// New message type for agent status updates
#[derive(Message)]
#[rtype(result = "()")]
pub struct AgentStatusUpdate {
    pub agent_id: String,
    pub connected: bool,
    pub last_seen: Instant,
}

// Add new message types for router integration
#[derive(Message)]
#[rtype(result = "()")]
pub struct SetRouter {
    pub router: Addr<RouterActor>,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct RegisterClientWithRouter {
    pub client_id: Uuid,
    pub addr: Addr<ClientSessionActor>,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct UnregisterClientWithRouter {
    pub client_id: Uuid,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct RegisterAgentWithRouter {
    pub agent_id: String,
    pub addr: Addr<AgentActor>,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct UnregisterAgentWithRouter {
    pub agent_id: String,
}

// Client connection data storage
struct ClientData {
    addr: Addr<ClientSessionActor>,
    connected: bool,
    last_seen: Instant,
    authenticated: bool,
    wallet_address: Option<String>,
}

// Agent connection data storage
struct AgentData {
    addr: Addr<AgentActor>,
    connected: bool,
    last_seen: Instant,
}

/// Enhanced actor for managing global system state
pub struct StateManagerActor {
    clients: DashMap<Uuid, ClientData>,
    agents: DashMap<String, AgentData>,
    router: Option<Addr<RouterActor>>,
}

impl StateManagerActor {
    pub fn new() -> Self {
        Self {
            clients: DashMap::new(),
            agents: DashMap::new(),
            router: None,
        }
    }
    
    // Set router reference
    pub fn set_router(&mut self, router_addr: Addr<RouterActor>) {
        self.router = Some(router_addr);
    }
    
    // Initialize cleanup task
    fn start_cleanup_task(&self, ctx: &mut Context<Self>) {
        // Run cleanup every minute to remove stale connections
        ctx.run_interval(Duration::from_secs(60), |act, _ctx| {
            let now = Instant::now();
            let timeout = Duration::from_secs(120); // 2 minutes timeout
            
            // Clean up disconnected clients
            let mut to_remove_clients = Vec::new();
            for entry in act.clients.iter() {
                let client_id = *entry.key();
                let client_data = entry.value();
                
                if !client_data.connected && now.duration_since(client_data.last_seen) > timeout {
                    to_remove_clients.push(client_id);
                }
            }
            
            // Remove stale clients
            for client_id in to_remove_clients {
                act.clients.remove(&client_id);
                tracing::info!("Removed stale client: {}", client_id);
            }
            
            // Clean up disconnected agents
            let mut to_remove_agents = Vec::new();
            for entry in act.agents.iter() {
                let agent_id = entry.key().clone();
                let agent_data = entry.value();
                
                if !agent_data.connected && now.duration_since(agent_data.last_seen) > timeout {
                    to_remove_agents.push(agent_id);
                }
            }
            
            // Remove stale agents
            for agent_id in to_remove_agents {
                act.agents.remove(&agent_id);
                tracing::info!("Removed stale agent: {}", agent_id);
            }
        });
    }
}

impl Actor for StateManagerActor {
    type Context = Context<Self>;
    
    fn started(&mut self, ctx: &mut Self::Context) {
        // Start the cleanup task
        self.start_cleanup_task(ctx);
    }
}

impl Handler<RegisterClient> for StateManagerActor {
    type Result = ();
    
    fn handle(&mut self, msg: RegisterClient, _ctx: &mut Self::Context) -> Self::Result {
        // Check if client already exists
        if let Some(mut entry) = self.clients.get_mut(&msg.client_id) {
            // Update existing client entry
            entry.addr = msg.addr.clone();
            entry.connected = true;
            entry.last_seen = Instant::now();
            
            tracing::info!("Client reconnected: {}", msg.client_id);
        } else {
            // Create new client entry
            let client_data = ClientData {
                addr: msg.addr.clone(),
                connected: true,
                last_seen: Instant::now(),
                authenticated: false,
                wallet_address: None,
            };
            
            self.clients.insert(msg.client_id, client_data);
            tracing::info!("Client registered: {}", msg.client_id);
        }
        
        // Update router if available
        if let Some(router) = &self.router {
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
        // Mark client as disconnected but keep in map for reconnection
        if let Some(mut entry) = self.clients.get_mut(&msg.client_id) {
            entry.connected = false;
            entry.last_seen = Instant::now();
            
            tracing::info!("Client disconnected: {}", msg.client_id);
        }
        
        // Update router if available
        if let Some(router) = &self.router {
            router.do_send(super::router_actor::UnregisterClient {
                client_id: msg.client_id,
            });
        }
    }
}

impl Handler<RegisterAgent> for StateManagerActor {
    type Result = ();
    
    fn handle(&mut self, msg: RegisterAgent, _ctx: &mut Self::Context) -> Self::Result {
        // Check if agent already exists
        if let Some(mut entry) = self.agents.get_mut(&msg.agent_id) {
            // Update existing agent entry
            entry.addr = msg.addr.clone();
            entry.connected = true;
            entry.last_seen = Instant::now();
            
            tracing::info!("Agent reconnected: {}", msg.agent_id);
        } else {
            // Create new agent entry
            let agent_data = AgentData {
                addr: msg.addr.clone(),
                connected: true,
                last_seen: Instant::now(),
            };
            
            self.agents.insert(msg.agent_id.clone(), agent_data);
            tracing::info!("Agent registered: {}", msg.agent_id);
        }
        
        // Update router if available
        if let Some(router) = &self.router {
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
        // Mark agent as disconnected but keep in map for reconnection
        if let Some(mut entry) = self.agents.get_mut(&msg.agent_id) {
            entry.connected = false;
            entry.last_seen = Instant::now();
            
            tracing::info!("Agent disconnected: {}", msg.agent_id);
        }
        
        // Update router if available
        if let Some(router) = &self.router {
            router.do_send(super::router_actor::UnregisterAgent {
                agent_id: msg.agent_id,
            });
        }
    }
}

impl Handler<ClientStatusUpdate> for StateManagerActor {
    type Result = ();
    
    fn handle(&mut self, msg: ClientStatusUpdate, _ctx: &mut Self::Context) -> Self::Result {
        // Update client connection status
        if let Some(mut entry) = self.clients.get_mut(&msg.client_id) {
            entry.connected = msg.connected;
            entry.last_seen = msg.last_seen;
            
            tracing::debug!("Client status updated: {} (connected: {})", 
                          msg.client_id, msg.connected);
        }
    }
}

impl Handler<AgentStatusUpdate> for StateManagerActor {
    type Result = ();
    
    fn handle(&mut self, msg: AgentStatusUpdate, _ctx: &mut Self::Context) -> Self::Result {
        // Update agent connection status
        if let Some(mut entry) = self.agents.get_mut(&msg.agent_id) {
            entry.connected = msg.connected;
            entry.last_seen = msg.last_seen;
            
            tracing::debug!("Agent status updated: {} (connected: {})", 
                          msg.agent_id, msg.connected);
        }
    }
}

impl Handler<SetRouter> for StateManagerActor {
    type Result = ();
    
    fn handle(&mut self, msg: SetRouter, _ctx: &mut Self::Context) -> Self::Result {
        self.router = Some(msg.router);
    }
}

