// WebSocket Server - actors/state_manager.rs
// my-actix-system/websocket-server/src/actors/state_manager.rs
use actix::{Actor, Context, Handler, Message, Addr};
use dashmap::DashMap;
use uuid::Uuid;
use super::client_session_actor::ClientSessionActor;
use super::agent_actor::AgentActor;

/// Messages for the state manager
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

/// Actor for managing global system state
pub struct StateManagerActor {
    clients: DashMap<Uuid, Addr<ClientSessionActor>>,
    agents: DashMap<String, Addr<AgentActor>>,
}

impl StateManagerActor {
    pub fn new() -> Self {
        Self {
            clients: DashMap::new(),
            agents: DashMap::new(),
        }
    }
}

impl Actor for StateManagerActor {
    type Context = Context<Self>;
}

impl Handler<RegisterClient> for StateManagerActor {
    type Result = ();
    
    fn handle(&mut self, msg: RegisterClient, _ctx: &mut Self::Context) -> Self::Result {
        self.clients.insert(msg.client_id, msg.addr);
        tracing::info!("Client registered: {}", msg.client_id);
    }
}

impl Handler<UnregisterClient> for StateManagerActor {
    type Result = ();
    
    fn handle(&mut self, msg: UnregisterClient, _ctx: &mut Self::Context) -> Self::Result {
        self.clients.remove(&msg.client_id);
        tracing::info!("Client unregistered: {}", msg.client_id);
    }
}

impl Handler<RegisterAgent> for StateManagerActor {
    type Result = ();
    
    fn handle(&mut self, msg: RegisterAgent, _ctx: &mut Self::Context) -> Self::Result {
        self.agents.insert(msg.agent_id.clone(), msg.addr);
        tracing::info!("Agent registered: {}", msg.agent_id);
    }
}

impl Handler<UnregisterAgent> for StateManagerActor {
    type Result = ();
    
    fn handle(&mut self, msg: UnregisterAgent, _ctx: &mut Self::Context) -> Self::Result {
        self.agents.remove(&msg.agent_id);
        tracing::info!("Agent unregistered: {}", msg.agent_id);
    }
}