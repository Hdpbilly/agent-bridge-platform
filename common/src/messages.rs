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
    // Added fields for delivery confirmation - optional with default values
    // to maintain backward compatibility
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default)]
    pub requires_ack: bool,
}

/// Message from agent to client(s)
#[derive(Debug, Clone, Serialize, Deserialize, Message)]
#[rtype(result = "()")]
pub struct AgentMessage {
    pub target_client_id: Option<Uuid>, // None means broadcast
    pub content: String,
    pub timestamp: u64,
    // Added fields for delivery confirmation - optional with default values
    // to maintain backward compatibility
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<u64>,
    #[serde(default)]
    pub requires_ack: bool,
    // Added field for message type classification (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_type: Option<String>,
}

/// New message acknowledgement type
#[derive(Debug, Clone, Serialize, Deserialize, Message)]
#[rtype(result = "()")]
pub struct MessageAcknowledgement {
    pub source_id: String, // Client ID or agent ID as string
    pub message_id: u64,
    pub timestamp: u64,
    pub status: AckStatus,
}

/// Acknowledgement status enum
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AckStatus {
    Received,
    Processed,
    Error(String),
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
    // Added new variants for session management and metrics
    // These won't break existing code since match statements
    // for SystemMessage should handle unknown variants
    SessionCreated {
        client_id: Uuid,
        session_id: String,
    },
    SessionRestored {
        client_id: Uuid,
        session_id: String,
    },
    SessionExpired {
        client_id: Uuid,
        session_id: String,
    },
    MetricsReport {
        connections: usize,
        messages_processed: u64,
        messages_per_second: f64,
        bytes_transferred: u64,
        timestamp: u64,
    },
}

// Helper functions for message size calculation (useful for metrics)
/// Helper trait for estimating message size for metrics
pub trait MessageSize {
    /// Estimate the size in bytes of the message
    fn size_bytes(&self) -> usize;
}

impl MessageSize for ClientMessage {
    fn size_bytes(&self) -> usize {
        // Approximate size calculation for metrics
        let mut size = self.content.len();
        size += 16; // Uuid size
        if let Some(ref wallet) = self.wallet_address {
            size += wallet.len();
        }
        size += 8; // timestamp
        if let Some(ref session_id) = self.session_id {
            size += session_id.len();
        }
        if self.message_id.is_some() {
            size += 8; // message_id
        }
        size
    }
}

impl MessageSize for AgentMessage {
    fn size_bytes(&self) -> usize {
        // Approximate size calculation
        let mut size = self.content.len();
        if self.target_client_id.is_some() {
            size += 16; // Uuid size
        }
        size += 8; // timestamp
        if let Some(ref msg_type) = self.message_type {
            size += msg_type.len();
        }
        if self.message_id.is_some() {
            size += 8; // message_id
        }
        size
    }
}

impl MessageSize for MessageAcknowledgement {
    fn size_bytes(&self) -> usize {
        // Approximate size
        let mut size = self.source_id.len();
        size += 8; // message_id
        size += 8; // timestamp
        // Add status size - rough estimate
        match &self.status {
            AckStatus::Received => size += 8,
            AckStatus::Processed => size += 9,
            AckStatus::Error(msg) => size += 5 + msg.len(),
        }
        size
    }
}