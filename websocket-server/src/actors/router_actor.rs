// websocket-server/src/actors/router_actor.rs
use actix::{Actor, Context, Handler, Message, Addr};
use uuid::Uuid;
use dashmap::DashMap;
use super::client_session_actor::ClientSessionActor;
use super::agent_actor::AgentActor;
use common::{ClientMessage, AgentMessage, SystemMessage};

// Message to send to a ClientSessionActor - actor-specific, so kept here
#[derive(Message)]
#[rtype(result = "()")]
pub struct ClientActorMessage {
    pub content: String,
}

// Message to send to an AgentActor - actor-specific, so kept here
#[derive(Message)]
#[rtype(result = "()")]
pub struct AgentActorMessage {
    pub content: String,
}

// Registration message types - actor-specific, so kept here
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

// Router actor for message routing
pub struct RouterActor {
    clients: DashMap<Uuid, Addr<ClientSessionActor>>,
    agents: DashMap<String, Addr<AgentActor>>,
    default_agent_id: Option<String>, // Default agent for Phase 2
}

impl RouterActor {
    pub fn new() -> Self {
        Self {
            clients: DashMap::new(),
            agents: DashMap::new(),
            default_agent_id: Some("agent1".to_string()), // Hardcoded for Phase 2
        }
    }
    
    // Register client address
    pub fn register_client(&self, client_id: Uuid, addr: Addr<ClientSessionActor>) {
        self.clients.insert(client_id, addr);
        tracing::info!("Client registered with router: {}", client_id);
    }
    
    // Unregister client
    pub fn unregister_client(&self, client_id: &Uuid) {
        self.clients.remove(client_id);
        tracing::info!("Client unregistered from router: {}", client_id);
    }
    
    // Register agent address
    pub fn register_agent(&self, agent_id: String, addr: Addr<AgentActor>) {
        self.agents.insert(agent_id.clone(), addr);
        tracing::info!("Agent registered with router: {}", agent_id);
    }
    
    // Unregister agent
    pub fn unregister_agent(&self, agent_id: &str) {
        self.agents.remove(agent_id);
        tracing::info!("Agent unregistered from router: {}", agent_id);
    }
    
    // Get the default agent for Phase 2
    fn get_default_agent(&self) -> Option<Addr<AgentActor>> {
        if let Some(id) = &self.default_agent_id {
            if let Some(entry) = self.agents.get(id) {
                return Some(entry.value().clone());
            }
        }
        None
    }
}

impl Actor for RouterActor {
    type Context = Context<Self>;
    
    fn started(&mut self, _ctx: &mut Self::Context) {
        tracing::info!("RouterActor started");
    }
    
    fn stopped(&mut self, _ctx: &mut Self::Context) {
        tracing::info!("RouterActor stopped");
    }
}

// Handle ClientMessage directly
impl Handler<ClientMessage> for RouterActor {
    type Result = ();
    
    fn handle(&mut self, msg: ClientMessage, _ctx: &mut Self::Context) -> Self::Result {
        tracing::info!("Routing client message from {}", msg.client_id);
        
        // In Phase 2, we route to the default agent if available
        if let Some(default_agent) = self.get_default_agent() {
            match serde_json::to_string(&msg) {
                Ok(content) => {
                    let agent_message = AgentActorMessage { content };
                    
                    if let Err(e) = default_agent.try_send(agent_message) {
                        tracing::error!("Failed to send message to default agent: {}", e);
                    }
                },
                Err(e) => tracing::error!("Failed to serialize client message: {}", e)
            }
        } else {
            // Try each agent if no default is set
            let mut sent = false;
            
            for agent_entry in self.agents.iter() {
                if let Ok(content) = serde_json::to_string(&msg) {
                    let agent_message = AgentActorMessage { content };
                    
                    if agent_entry.value().try_send(agent_message).is_ok() {
                        sent = true;
                    }
                }
            }
            
            if !sent {
                tracing::warn!("No agents available to receive message from client {}", msg.client_id);
            }
        }
    }
}

// Handle AgentMessage directly
impl Handler<AgentMessage> for RouterActor {
    type Result = ();
    
    fn handle(&mut self, msg: AgentMessage, _ctx: &mut Self::Context) -> Self::Result {
        match msg.target_client_id {
            Some(client_id) => {
                // Direct message to specific client
                tracing::info!("Routing agent message to client {}", client_id);
                
                if let Some(client_entry) = self.clients.get(&client_id) {
                    if let Ok(content) = serde_json::to_string(&msg) {
                        let client_message = ClientActorMessage { content };
                        
                        if let Err(e) = client_entry.value().try_send(client_message) {
                            tracing::error!("Failed to deliver message to client {}: {}", client_id, e);
                        } else {
                            tracing::debug!("Message delivered to client {}", client_id);
                        }
                    } else {
                        tracing::error!("Failed to serialize agent message for client {}", client_id);
                    }
                } else {
                    tracing::warn!("Client {} not found for message delivery", client_id);
                }
            },
            None => {
                // Broadcast to all clients
                tracing::info!("Broadcasting agent message to all clients");
                
                if let Ok(content) = serde_json::to_string(&msg) {
                    let mut sent_count = 0;
                    let total_count = self.clients.len();
                    
                    for client_entry in self.clients.iter() {
                        let client_message = ClientActorMessage { content: content.clone() };
                        
                        if client_entry.value().try_send(client_message).is_ok() {
                            sent_count += 1;
                        }
                    }
                    
                    tracing::info!("Broadcast delivered to {}/{} clients", sent_count, total_count);
                } else {
                    tracing::error!("Failed to serialize agent broadcast message");
                }
            }
        }
    }
}

// Handle SystemMessage
impl Handler<SystemMessage> for RouterActor {
    type Result = ();
    
    fn handle(&mut self, msg: SystemMessage, _ctx: &mut Self::Context) -> Self::Result {
        match &msg {
            SystemMessage::ClientConnected { client_id, authenticated, wallet_address } => {
                tracing::info!(
                    "System message: Client connected - ID: {}, Authenticated: {}", 
                    client_id, authenticated
                );
                
                // Notify agents about client connection
                if let Some(default_agent) = self.get_default_agent() {
                    if let Ok(content) = serde_json::to_string(&msg) {
                        let agent_message = AgentActorMessage { content };
                        let _ = default_agent.try_send(agent_message);
                    }
                }
            },
            SystemMessage::ClientDisconnected { client_id } => {
                tracing::info!("System message: Client disconnected - ID: {}", client_id);
                
                // Notify agents about client disconnection
                if let Some(default_agent) = self.get_default_agent() {
                    if let Ok(content) = serde_json::to_string(&msg) {
                        let agent_message = AgentActorMessage { content };
                        let _ = default_agent.try_send(agent_message);
                    }
                }
            },
            _ => {
                // Handle other system messages
                tracing::debug!("System message: {:?}", msg);
            }
        }
    }
}

// Registration handlers
impl Handler<RegisterClient> for RouterActor {
    type Result = ();
    
    fn handle(&mut self, msg: RegisterClient, _ctx: &mut Self::Context) -> Self::Result {
        self.register_client(msg.client_id, msg.addr);
    }
}

impl Handler<UnregisterClient> for RouterActor {
    type Result = ();
    
    fn handle(&mut self, msg: UnregisterClient, _ctx: &mut Self::Context) -> Self::Result {
        self.unregister_client(&msg.client_id);
    }
}

impl Handler<RegisterAgent> for RouterActor {
    type Result = ();
    
    fn handle(&mut self, msg: RegisterAgent, _ctx: &mut Self::Context) -> Self::Result {
        self.register_agent(msg.agent_id, msg.addr);
    }
}

impl Handler<UnregisterAgent> for RouterActor {
    type Result = ();
    
    fn handle(&mut self, msg: UnregisterAgent, _ctx: &mut Self::Context) -> Self::Result {
        self.unregister_agent(&msg.agent_id);
    }
}