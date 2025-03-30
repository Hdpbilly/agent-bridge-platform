// websocket-server/src/actors/router_actor.rs
use actix::{Actor, Context, Handler, Message, Addr};
use uuid::Uuid;
use dashmap::DashMap;
use super::client_session_actor::ClientSessionActor;
use super::agent_actor::AgentActor;
use common::{ClientMessage, AgentMessage};

// Define specific message types for different actors

// Message to send to a ClientSessionActor
#[derive(Message)]
#[rtype(result = "()")]
pub struct ClientActorMessage {
    pub content: String,
}

// Message to send to an AgentActor
#[derive(Message)]
#[rtype(result = "()")]
pub struct AgentActorMessage {
    pub content: String,
}

/// Enhanced message for routing with reconnection support
#[derive(Message)]
#[rtype(result = "()")]
pub struct RouteMessage {
    pub message: String,
    pub from_client_id: Option<Uuid>,
    pub to_client_id: Option<Uuid>, // None means broadcast
    pub require_delivery: bool,     // If true, buffer for disconnected recipients
}

/// Message for client registration
#[derive(Message)]
#[rtype(result = "()")]
pub struct RegisterClient {
    pub client_id: Uuid,
    pub addr: Addr<ClientSessionActor>,
}

/// Message for client unregistration
#[derive(Message)]
#[rtype(result = "()")]
pub struct UnregisterClient {
    pub client_id: Uuid,
}

/// Message for agent registration
#[derive(Message)]
#[rtype(result = "()")]
pub struct RegisterAgent {
    pub agent_id: String,
    pub addr: Addr<AgentActor>,
}

/// Message for agent unregistration
#[derive(Message)]
#[rtype(result = "()")]
pub struct UnregisterAgent {
    pub agent_id: String,
}

/// Enhanced router actor with client/agent lookup
pub struct RouterActor {
    clients: DashMap<Uuid, Addr<ClientSessionActor>>,
    agents: DashMap<String, Addr<AgentActor>>,
}

impl RouterActor {
    pub fn new() -> Self {
        Self {
            clients: DashMap::new(),
            agents: DashMap::new(),
        }
    }
    
    // Register client address
    pub fn register_client(&self, client_id: Uuid, addr: Addr<ClientSessionActor>) {
        self.clients.insert(client_id, addr);
    }
    
    // Unregister client
    pub fn unregister_client(&self, client_id: &Uuid) {
        self.clients.remove(client_id);
    }
    
    // Register agent address
    pub fn register_agent(&self, agent_id: String, addr: Addr<AgentActor>) {
        self.agents.insert(agent_id, addr);
    }
    
    // Unregister agent
    pub fn unregister_agent(&self, agent_id: &str) {
        self.agents.remove(agent_id);
    }
}

impl Actor for RouterActor {
    type Context = Context<Self>;
}

impl Handler<RouteMessage> for RouterActor {
    type Result = ();
    
    fn handle(&mut self, msg: RouteMessage, _ctx: &mut Self::Context) -> Self::Result {
        match (msg.from_client_id, msg.to_client_id) {
            (Some(from), None) => {
                // Client to Agent (broadcast)
                tracing::info!("Routing message from client {} to agent", from);
                
                // In Phase 2, we route to all agents
                for agent_entry in self.agents.iter() {
                    let agent_addr = agent_entry.value();
                    
                    // Parse as ClientMessage
                    if let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&msg.message) {
                        // Forward to agent using proper message type
                        let agent_message = AgentActorMessage {
                            content: serde_json::to_string(&client_msg).unwrap_or_default(),
                        };
                        
                        // Use do_send which doesn't require response handling
                        agent_addr.do_send(agent_message);
                    }
                }
            },
            (None, Some(to)) => {
                // Agent to specific client
                tracing::info!("Routing message from agent to client {}", to);
                
                // Look up client
                if let Some(client_entry) = self.clients.get(&to) {
                    let client_addr = client_entry.value();
                    
                    // Forward message directly with proper message type
                    let client_message = ClientActorMessage {
                        content: msg.message.clone(),
                    };
                    
                    client_addr.do_send(client_message);
                } else if msg.require_delivery {
                    // Client not connected but delivery required
                    // In a full implementation, we would buffer in a persistent store
                    tracing::warn!("Client {} not connected, message buffering not implemented", to);
                }
            },
            (None, None) => {
                // Agent broadcast to all clients
                tracing::info!("Broadcasting message from agent to all clients");
                
                // Parse as AgentMessage
                if let Ok(agent_msg) = serde_json::from_str::<AgentMessage>(&msg.message) {
                    // Broadcast to all clients
                    for client_entry in self.clients.iter() {
                        let client_addr = client_entry.value();
                        
                        // Use proper message type
                        let client_message = ClientActorMessage {
                            content: serde_json::to_string(&agent_msg).unwrap_or_default(),
                        };
                        
                        client_addr.do_send(client_message);
                    }
                }
            },
            (Some(from), Some(to)) => {
                // Client to specific client (now supported in Phase 2)
                tracing::info!("Routing message from client {} to client {}", from, to);
                
                // Look up target client
                if let Some(client_entry) = self.clients.get(&to) {
                    let client_addr = client_entry.value();
                    
                    // Forward message directly with proper message type
                    let client_message = ClientActorMessage {
                        content: msg.message.clone(),
                    };
                    
                    client_addr.do_send(client_message);
                } else if msg.require_delivery {
                    // Client not connected but delivery required
                    tracing::warn!("Target client {} not connected, message buffering not implemented", to);
                }
            }
        }
    }
}

// Implement handlers for client/agent registration
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