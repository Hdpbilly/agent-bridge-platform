// web-server/src/client_registry.rs
use actix::{Actor, Context, Handler, Message, Addr, AsyncContext, MessageResult};
use chrono::Utc;
use common::models::session::{ClientSession, SessionResult};
use dashmap::DashMap;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;
use crate::utils::token::create_session_token;

// Default session TTL in seconds (24 hours)
const DEFAULT_SESSION_TTL: i64 = 86400;

/// Actor message: Register a new anonymous client
#[derive(Message)]
#[rtype(result = "(Uuid, String)")]
pub struct RegisterAnonymousClient;

/// Actor message: Get a client session by session token
#[derive(Message)]
#[rtype(result = "SessionResult")]
pub struct GetClientSession {
    pub session_token: String,
}

/// Actor message: Get a client session by client ID
#[derive(Message)]
#[rtype(result = "SessionResult")]
pub struct GetClientSessionById {
    pub client_id: Uuid,
}

/// Actor message: Update client session activity
#[derive(Message)]
#[rtype(result = "()")]
pub struct UpdateSessionActivity {
    pub session_token: String,
}

/// Actor message: Invalidate/remove a client session
#[derive(Message)]
#[rtype(result = "bool")]
pub struct InvalidateClientSession {
    pub session_token: String,
}

/// Actor message: Update a client session
#[derive(Message)]
#[rtype(result = "SessionResult")]
pub struct UpdateClientSession {
    pub session_token: String,
    pub is_authenticated: Option<bool>,
    pub wallet_address: Option<Option<String>>,
    pub metadata: Option<std::collections::HashMap<String, String>>,
    pub extend_ttl: bool,
}

/// Actor message: Clean up expired sessions
#[derive(Message)]
#[rtype(result = "usize")]
pub struct CleanupExpiredSessions;

/// Actor message: Get session metrics
#[derive(Message)]
#[rtype(result = "SessionMetrics")]
pub struct GetSessionMetrics;

/// Session metrics
#[derive(Debug, Clone)]
pub struct SessionMetrics {
    pub total_sessions: usize,
    pub anonymous_sessions: usize,
    pub authenticated_sessions: usize,
    pub expired_count: usize,
    pub avg_session_age_seconds: f64,
}

/// ClientRegistryActor for managing client sessions
pub struct ClientRegistryActor {
    // Map from session token to session data
    sessions: Arc<DashMap<String, ClientSession>>,
    // Map from client ID to session token
    client_lookup: Arc<DashMap<Uuid, String>>,
    // Session TTL in seconds
    session_ttl: i64,
    // Cleanup interval in seconds
    cleanup_interval: u64,
    // Metrics
    metrics: SessionMetrics,
}

impl Default for ClientRegistryActor {
    fn default() -> Self {
        Self::new()
    }
}

impl ClientRegistryActor {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(DashMap::new()),
            client_lookup: Arc::new(DashMap::new()),
            session_ttl: DEFAULT_SESSION_TTL,
            cleanup_interval: 3600, // Run cleanup every hour
            metrics: SessionMetrics {
                total_sessions: 0,
                anonymous_sessions: 0,
                authenticated_sessions: 0,
                expired_count: 0,
                avg_session_age_seconds: 0.0,
            },
        }
    }
    
    pub fn with_ttl(mut self, ttl_seconds: i64) -> Self {
        self.session_ttl = ttl_seconds;
        self
    }
    
    pub fn with_cleanup_interval(mut self, interval_seconds: u64) -> Self {
        self.cleanup_interval = interval_seconds;
        self
    }
    
    /// Update session metrics
    fn update_metrics(&mut self) {
        let mut anonymous_count = 0;
        let mut authenticated_count = 0;
        let mut age_sum = 0.0;
        
        for entry in self.sessions.iter() {
            let session = entry.value();
            if session.is_authenticated {
                authenticated_count += 1;
            } else {
                anonymous_count += 1;
            }
            
            // Calculate session age in seconds
            let age = Utc::now().signed_duration_since(session.created_at).num_seconds() as f64;
            age_sum += age;
        }
        
        let total = anonymous_count + authenticated_count;
        
        self.metrics = SessionMetrics {
            total_sessions: total,
            anonymous_sessions: anonymous_count,
            authenticated_sessions: authenticated_count,
            expired_count: self.metrics.expired_count,
            avg_session_age_seconds: if total > 0 { age_sum / total as f64 } else { 0.0 },
        };
    }
    
    /// Remove expired sessions and update metrics
    fn cleanup_sessions(&mut self) -> usize {
        let now = Utc::now();
        let mut expired_count = 0;
        
        // Collect expired session tokens
        let expired_tokens: Vec<String> = self.sessions.iter()
            .filter_map(|entry| {
                let session = entry.value();
                let age = now.signed_duration_since(session.last_active);
                if age.num_seconds() > self.session_ttl {
                    Some(session.session_token.clone())
                } else {
                    None
                }
            })
            .collect();
        
        // Remove expired sessions
        for token in expired_tokens {
            if let Some(session) = self.sessions.remove(&token) {
                self.client_lookup.remove(&session.1.client_id);
                expired_count += 1;
            }
        }
        
        // Update metrics
        self.metrics.expired_count += expired_count;
        self.update_metrics();
        
        expired_count
    }
}

impl Actor for ClientRegistryActor {
    type Context = Context<Self>;
    
    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!("ClientRegistryActor started with TTL: {}s", self.session_ttl);
        
        // Schedule periodic session cleanup
        ctx.run_interval(Duration::from_secs(self.cleanup_interval), |act, _ctx| {
            let expired_count = act.cleanup_sessions();
            if expired_count > 0 {
                tracing::info!("Cleaned up {} expired sessions", expired_count);
            }
        });
    }
    
    fn stopped(&mut self, _ctx: &mut Self::Context) {
        tracing::info!(
            "ClientRegistryActor stopped. Final metrics: {} total sessions, {} expired during lifetime",
            self.metrics.total_sessions,
            self.metrics.expired_count
        );
    }
}

// Handle registration of a new anonymous client
impl Handler<RegisterAnonymousClient> for ClientRegistryActor {
    type Result = MessageResult<RegisterAnonymousClient>;
    
    fn handle(&mut self, _msg: RegisterAnonymousClient, _ctx: &mut Self::Context) -> Self::Result {
        let client_id = Uuid::new_v4();
        // Use the secure token generator
        let session_token = create_session_token();
        
        let session = ClientSession::new_anonymous(client_id, session_token.clone());
        
        // Store session data
        self.sessions.insert(session_token.clone(), session);
        self.client_lookup.insert(client_id, session_token.clone());
        
        // Update metrics
        self.metrics.anonymous_sessions += 1;
        self.metrics.total_sessions += 1;
        
        tracing::info!("Registered new anonymous client: {}", client_id);
        
        MessageResult((client_id, session_token))
    }
}

// Handle retrieval of a client session by token
impl Handler<GetClientSession> for ClientRegistryActor {
    type Result = MessageResult<GetClientSession>;
    
    fn handle(&mut self, msg: GetClientSession, _ctx: &mut Self::Context) -> Self::Result {
        let result = if let Some(mut entry) = self.sessions.get_mut(&msg.session_token) {
            let session = entry.value_mut();
            
            // Check if session has expired
            if session.is_expired(self.session_ttl) {
                tracing::debug!("Session expired: {}", session.client_id);
                SessionResult::Expired
            } else {
                // Update activity timestamp
                session.update_activity();
                
                tracing::debug!("Retrieved session for client: {}", session.client_id);
                SessionResult::Success(session.clone())
            }
        } else {
            tracing::debug!("Session not found for token: {}", msg.session_token);
            SessionResult::NotFound
        };
        
        MessageResult(result)
    }
}

// Handle retrieval of a client session by client ID
impl Handler<GetClientSessionById> for ClientRegistryActor {
    type Result = MessageResult<GetClientSessionById>;
    
    fn handle(&mut self, msg: GetClientSessionById, _ctx: &mut Self::Context) -> Self::Result {
        let result = if let Some(token_entry) = self.client_lookup.get(&msg.client_id) {
            let token = token_entry.value();
            
            if let Some(mut session_entry) = self.sessions.get_mut(token) {
                let session = session_entry.value_mut();
                
                // Check if session has expired
                if session.is_expired(self.session_ttl) {
                    tracing::debug!("Session expired: {}", session.client_id);
                    SessionResult::Expired
                } else {
                    // Update activity timestamp
                    session.update_activity();
                    
                    tracing::debug!("Retrieved session for client: {}", session.client_id);
                    SessionResult::Success(session.clone())
                }
            } else {
                SessionResult::NotFound
            }
        } else {
            tracing::debug!("Session not found for client ID: {}", msg.client_id);
            SessionResult::NotFound
        };
        
        MessageResult(result)
    }
}

// Handle session activity updates
impl Handler<UpdateSessionActivity> for ClientRegistryActor {
    type Result = ();
    
    fn handle(&mut self, msg: UpdateSessionActivity, _ctx: &mut Self::Context) -> Self::Result {
        if let Some(mut entry) = self.sessions.get_mut(&msg.session_token) {
            entry.value_mut().update_activity();
            tracing::trace!("Updated activity for session: {}", entry.value().client_id);
        }
    }
}

// Handle session invalidation
impl Handler<InvalidateClientSession> for ClientRegistryActor {
    type Result = MessageResult<InvalidateClientSession>;
    
    fn handle(&mut self, msg: InvalidateClientSession, _ctx: &mut Self::Context) -> Self::Result {
        let result = if let Some((_, session)) = self.sessions.remove(&msg.session_token) {
            self.client_lookup.remove(&session.client_id);
            
            // Update metrics
            if session.is_authenticated {
                self.metrics.authenticated_sessions -= 1;
            } else {
                self.metrics.anonymous_sessions -= 1;
            }
            self.metrics.total_sessions -= 1;
            
            tracing::info!("Invalidated session for client: {}", session.client_id);
            true
        } else {
            false
        };
        
        MessageResult(result)
    }
}

// Handle session updates
impl Handler<UpdateClientSession> for ClientRegistryActor {
    type Result = MessageResult<UpdateClientSession>;
    
    fn handle(&mut self, msg: UpdateClientSession, _ctx: &mut Self::Context) -> Self::Result {
        let result = if let Some(mut entry) = self.sessions.get_mut(&msg.session_token) {
            let session = entry.value_mut();
            
            // Check if session has expired
            if session.is_expired(self.session_ttl) {
                tracing::debug!("Session expired: {}", session.client_id);
                SessionResult::Expired
            } else {
                // Track authentication status change for metrics
                let was_authenticated = session.is_authenticated;
                
                // Update session fields
                if let Some(is_authenticated) = msg.is_authenticated {
                    session.is_authenticated = is_authenticated;
                }
                
                if let Some(wallet_address) = msg.wallet_address {
                    session.wallet_address = wallet_address;
                }
                
                if let Some(metadata) = msg.metadata {
                    for (key, value) in metadata {
                        session.set_metadata(key, value);
                    }
                }
                
                // Update activity timestamp
                session.update_activity();
                
                // Update metrics if authentication status changed
                if !was_authenticated && session.is_authenticated {
                    self.metrics.anonymous_sessions -= 1;
                    self.metrics.authenticated_sessions += 1;
                    tracing::info!("Client upgraded to authenticated status: {}", session.client_id);
                } else if was_authenticated && !session.is_authenticated {
                    self.metrics.authenticated_sessions -= 1;
                    self.metrics.anonymous_sessions += 1;
                    tracing::info!("Client downgraded to anonymous status: {}", session.client_id);
                }
                
                tracing::debug!("Updated session for client: {}", session.client_id);
                SessionResult::Success(session.clone())
            }
        } else {
            tracing::debug!("Session not found for token: {}", msg.session_token);
            SessionResult::NotFound
        };
        
        MessageResult(result)
    }
}

// Handle session cleanup
impl Handler<CleanupExpiredSessions> for ClientRegistryActor {
    type Result = MessageResult<CleanupExpiredSessions>;
    
    fn handle(&mut self, _msg: CleanupExpiredSessions, _ctx: &mut Self::Context) -> Self::Result {
        let expired_count = self.cleanup_sessions();
        tracing::info!("Cleaned up {} expired sessions", expired_count);
        MessageResult(expired_count)
    }
}

// Handle metrics request
impl Handler<GetSessionMetrics> for ClientRegistryActor {
    type Result = MessageResult<GetSessionMetrics>;
    
    fn handle(&mut self, _msg: GetSessionMetrics, _ctx: &mut Self::Context) -> Self::Result {
        // Update metrics before returning
        self.update_metrics();
        MessageResult(self.metrics.clone())
    }
}   