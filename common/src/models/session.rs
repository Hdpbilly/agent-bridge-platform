// common/src/models/session.rs
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;
use chrono::{DateTime, Utc};

/// Client session data structure for tracking both anonymous and authenticated users
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientSession {
    /// Unique client identifier
    pub client_id: Uuid,
    /// Secure session token used for cookie-based tracking
    pub session_token: String,
    /// Timestamp when the session was created
    pub created_at: DateTime<Utc>,
    /// Timestamp of last client activity
    pub last_active: DateTime<Utc>,
    /// Whether the client has been authenticated
    pub is_authenticated: bool,
    /// Wallet address for authenticated clients
    pub wallet_address: Option<String>,
    /// Arbitrary session data
    pub metadata: HashMap<String, String>,
}

impl ClientSession {
    /// Create a new anonymous client session
    pub fn new_anonymous(client_id: Uuid, session_token: String) -> Self {
        let now = Utc::now();
        Self {
            client_id,
            session_token,
            created_at: now,
            last_active: now,
            is_authenticated: false,
            wallet_address: None,
            metadata: HashMap::new(),
        }
    }
    
    /// Update session activity timestamp
    pub fn update_activity(&mut self) {
        self.last_active = Utc::now();
    }
    
    /// Check if the session has expired based on TTL
    pub fn is_expired(&self, ttl_seconds: i64) -> bool {
        let now = Utc::now();
        let age = now.signed_duration_since(self.last_active);
        age.num_seconds() > ttl_seconds
    }
    
    /// Upgrade session to authenticated status
    pub fn authenticate(&mut self, wallet_address: String) {
        self.is_authenticated = true;
        self.wallet_address = Some(wallet_address);
        self.update_activity();
    }
    
    /// Add or update metadata value
    pub fn set_metadata(&mut self, key: String, value: String) {
        self.metadata.insert(key, value);
    }
    
    /// Get metadata value
    pub fn get_metadata(&self, key: &str) -> Option<&String> {
        self.metadata.get(key)
    }
}

/// Result of session operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionResult {
    Success(ClientSession),
    NotFound,
    Expired,
    Invalid,
}

/// Response structure for session API endpoints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientSessionResponse {
    pub client_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub is_authenticated: bool,
    pub wallet_address: Option<String>,
    pub new_session: bool,
    // Omit sensitive data like session_token
}

impl From<&ClientSession> for ClientSessionResponse {
    fn from(session: &ClientSession) -> Self {
        Self {
            client_id: session.client_id,
            created_at: session.created_at,
            is_authenticated: session.is_authenticated,
            wallet_address: session.wallet_address.clone(),
            new_session: false, // Default value, should be overridden when appropriate
        }
    }
}