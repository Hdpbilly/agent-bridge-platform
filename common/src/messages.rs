// Common Crate - messages.rs
// my-actix-system/common/src/messages.rs
use actix::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Message from client to agent
#[derive(Debug, Clone, Serialize, Deserialize, Message)]
#[rtype(result = "()")]
pub struct ClientMessage {
    pub client_id: Uuid,
    pub content: String,
    pub authenticated: bool,
    pub wallet_address: Option<String>,
    pub timestamp: u64,
}

/// Message from agent to client(s)
#[derive(Debug, Clone, Serialize, Deserialize, Message)]
#[rtype(result = "()")]
pub struct AgentMessage {
    pub target_client_id: Option<Uuid>, // None means broadcast
    pub content: String,
    pub timestamp: u64,
}

/// System message for internal communication
#[derive(Debug, Clone, Serialize, Deserialize, Message)]
#[rtype(result = "()")]
pub enum SystemMessage {
    ClientConnected {
        client_id: Uuid,
        authenticated: bool,
        wallet_address: Option<String>,
    },
    ClientDisconnected {
        client_id: Uuid,
    },
    AgentConnected,
    AgentDisconnected,
    HeartbeatRequest,
    HeartbeatResponse,
}