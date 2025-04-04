// web-server/src/proxy.rs
use actix::{Actor, StreamHandler, AsyncContext, Context, ActorContext, Addr, Message, Handler};
use actix_web::{web, HttpRequest, HttpResponse, Error};
use actix_web_actors::ws;
use dashmap::DashMap;
use serde::{Serialize, Deserialize};
use tokio::sync::mpsc;
use futures::{StreamExt, SinkExt};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message as WsMessage};
use tokio_tungstenite::tungstenite::protocol::frame::CloseFrame;
use common::Config;
use common::models::session::SessionResult;
use tokio_tungstenite::tungstenite::error::Error as WsError;
use uuid::Uuid;
use std::time::{Duration, Instant};
use std::borrow::Cow;
use std::sync::Arc;
use std::convert::TryFrom;
use tungstenite::protocol::frame::coding::CloseCode as TungsteniteCloseCode;

use crate::client_registry::{ClientRegistryActor, GetClientSession, UpdateSessionActivity};

// Shared state for active WebSocket connections
pub struct ActiveConnections {
    // Maps session token to ProxyActor address
    connections: DashMap<String, Addr<ProxyActor>>,
}

impl ActiveConnections {
    pub fn new() -> Self {
        Self {
            connections: DashMap::new(),
        }
    }
    
    // Register a new connection
    pub fn register(&self, session_token: String, addr: Addr<ProxyActor>) -> Option<Addr<ProxyActor>> {
        self.connections.insert(session_token, addr)
    }
    
    // Unregister a connection
    pub fn unregister(&self, session_token: &str) -> bool {
        self.connections.remove(session_token).is_some()
    }
    
    // Get connection count
    pub fn count(&self) -> usize {
        self.connections.len()
    }
}

impl Default for ActiveConnections {
    fn default() -> Self {
        Self::new()
    }
}

// WsMessage types for proxy communication
#[derive(Message)]
#[rtype(result = "()")]
pub enum ProxyMessage {
    WebSocketMessage(String),
    WebSocketBinary(Vec<u8>),
    WebSocketPing,
    WebSocketPong,
    WebSocketClose,
    Disconnected,
}

// Enhanced ProxyActor with real proxying and session validation
pub struct ProxyActor {
    client_id: Uuid,
    session_token: Option<String>,
    ws_sink: Option<mpsc::Sender<WsMessage>>,
    last_heartbeat: Instant,
    reconnect_attempts: u32,
    ws_server_url: String,
    // Flag to track if we're connected to WebSocket server
    is_connected_to_server: bool,
    // Registry for client session validation
    registry: Option<Addr<ClientRegistryActor>>,
    // Reference to active connections for unregistering on stop
    active_connections: Option<web::Data<ActiveConnections>>,
}

impl ProxyActor {
    pub fn new(
        client_id: Uuid, 
        ws_server_url: String, 
        session_token: Option<String>,
        registry: Option<Addr<ClientRegistryActor>>,
        active_connections: Option<web::Data<ActiveConnections>>
    ) -> Self {
        Self { 
            client_id,
            session_token,
            ws_sink: None,
            last_heartbeat: Instant::now(),
            reconnect_attempts: 0,
            ws_server_url,
            is_connected_to_server: false,
            registry,
            active_connections,
        }
    }
    
    // Heartbeat to check client connection
    fn heartbeat(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(Duration::from_secs(5), |act, ctx| {
            if Instant::now().duration_since(act.last_heartbeat) > Duration::from_secs(30) {
                // Heartbeat timeout - attempt reconnection to WebSocket server
                tracing::warn!("Client heartbeat timeout: {}", act.client_id);
                
                // Close current connection if it exists
                if act.ws_sink.is_some() {
                    act.ws_sink = None;
                    act.is_connected_to_server = false;
                }
                
                // Calculate backoff for reconnection
                let backoff = std::cmp::min(
                    2u64.pow(act.reconnect_attempts),
                    60 // Cap at 60 seconds
                );
                
                ctx.run_later(Duration::from_secs(backoff), |act, ctx| {
                    tracing::info!("Attempting reconnection for client: {}", act.client_id);
                    act.connect_to_ws_server(ctx);
                });
                
                // Increment reconnect counter
                act.reconnect_attempts += 1;
                
                return;
            }
            
            // Regular ping
            ctx.ping(b"");
        });
    }
    
    // Connect to WebSocket server
    fn connect_to_ws_server(&mut self, ctx: &mut ws::WebsocketContext<Self>) {
        // Reset heartbeat
        self.last_heartbeat = Instant::now();
        
        // Create the WebSocket server URL with client ID
        let ws_url = format!("{}/ws/client/{}", self.ws_server_url, self.client_id);
        
        // Create channel for communication
        let (tx, mut rx) = mpsc::channel::<WsMessage>(100);
        self.ws_sink = Some(tx);
        
        // Get context address to communicate back
        let addr = ctx.address();
        
        // Spawn connection task
        let fut = async move {
            match connect_async(ws_url).await {
                Ok((ws_stream, _)) => {
                    let (mut ws_sink, mut ws_stream) = ws_stream.split();
                    
                    // Forward messages from client to WS server
                    tokio::spawn(async move {
                        while let Some(msg) = rx.recv().await {
                            if let Err(e) = ws_sink.send(msg).await {
                                tracing::error!("Error sending to WS server: {}", e);
                                break;
                            }
                        }
                    });
                    
                    // Forward messages from WS server to client
                    while let Some(msg) = ws_stream.next().await {
                        match msg {
                            Ok(WsMessage::Text(text)) => {
                                addr.do_send(ProxyMessage::WebSocketMessage(text));
                            },
                            Ok(WsMessage::Binary(data)) => {
                                addr.do_send(ProxyMessage::WebSocketBinary(data));
                            },
                            Ok(WsMessage::Ping(_)) => {
                                addr.do_send(ProxyMessage::WebSocketPing);
                            },
                            Ok(WsMessage::Pong(_)) => {
                                addr.do_send(ProxyMessage::WebSocketPong);
                            },
                            Ok(WsMessage::Close(_)) => {
                                addr.do_send(ProxyMessage::WebSocketClose);
                                break;
                            },
                            Ok(WsMessage::Frame(_)) => {
                                // Ignore low-level frames
                            },
                            Err(e) => {
                                tracing::error!("WebSocket error: {}", e);
                                addr.do_send(ProxyMessage::Disconnected);
                                break;
                            }
                        }
                    }
                    
                    // Connection closed
                    addr.do_send(ProxyMessage::Disconnected);
                },
                Err(e) => {
                    tracing::error!("Failed to connect to WebSocket server: {}", e);
                    addr.do_send(ProxyMessage::Disconnected);
                }
            }
        };
        
        // Spawn the future
        actix::spawn(fut);
    }
    
    // Update session activity
    fn update_session_activity(&self) {
        if let Some(session_token) = &self.session_token {
            if let Some(registry) = &self.registry {
                let token = session_token.clone();
                let registry_clone = registry.clone();
                actix::spawn(async move {
                    let _ = registry_clone.send(UpdateSessionActivity { session_token: token }).await;
                });
            }
        }
    }
}

impl Actor for ProxyActor {
    type Context = ws::WebsocketContext<Self>;
    
    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!("Proxy started for client: {}", self.client_id);
        
        // Setup heartbeat
        self.heartbeat(ctx);
        
        // Connect to WebSocket server
        self.connect_to_ws_server(ctx);
        self.is_connected_to_server = true;
        
        // Register connection with active connections if session token exists
        if let Some(token) = &self.session_token {
            if let Some(active_conns) = &self.active_connections {
                active_conns.register(token.clone(), ctx.address());
                tracing::info!("Registered connection for session token, active connections: {}", 
                             active_conns.count());
            }
        }
    }
    
    fn stopped(&mut self, _ctx: &mut Self::Context) {
        tracing::info!("Proxy stopped for client: {}", self.client_id);
        
        // Unregister from active connections if we have a session token
        if let Some(session_token) = &self.session_token {
            if let Some(active_conns) = &self.active_connections {
                active_conns.unregister(session_token);
                tracing::debug!("Unregistered from active connections: {}", self.client_id);
            }
        }
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for ProxyActor {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        // Update heartbeat timestamp
        self.last_heartbeat = Instant::now();
        
        match msg {
            Ok(ws::Message::Ping(msg)) => {
                // Respond to ping
                ctx.pong(&msg);
                
                // Forward ping to WebSocket server
                if let Some(tx) = &self.ws_sink {
                    let _ = tx.try_send(WsMessage::Ping(msg.to_vec()));
                }
            },
            Ok(ws::Message::Pong(_)) => {
                // Just update the heartbeat timestamp
            },
            Ok(ws::Message::Text(text)) => {
                // Forward text message to WebSocket server
                tracing::debug!("Forwarding message from client {} to server: {}", self.client_id, text);
                
                if let Some(tx) = &self.ws_sink {
                    let _ = tx.try_send(WsMessage::Text(text.to_string()));
                } else {
                    tracing::warn!("No WebSocket connection to forward message");
                    // Attempt reconnection
                    self.connect_to_ws_server(ctx);
                }
                
                // Update session activity
                self.update_session_activity();
            },
            Ok(ws::Message::Binary(bin)) => {
                // Forward binary message to WebSocket server
                if let Some(tx) = &self.ws_sink {
                    let _ = tx.try_send(WsMessage::Binary(bin.to_vec()));
                }
            }, 
            Ok(ws::Message::Close(reason)) => {
                if let Some(tx) = &self.ws_sink {
                    if let Some(ref r) = reason {
                        tracing::debug!(
                            "Client requested close: code={:?}, reason={:?}",
                            r.code,
                            r.description
                        );
                    }
                    let _ = tx.try_send(WsMessage::Close(None));
                }
                ctx.close(reason);
            },
            Ok(ws::Message::Continuation(_)) => {
                // Not handling continuation frames in this implementation
            },
            Ok(ws::Message::Nop) => {
                // No operation, nothing to do
            },
            Err(e) => {
                tracing::error!("WebSocket protocol error: {}", e);
            }
        }
    }
}

impl Handler<ProxyMessage> for ProxyActor {
    type Result = ();
    
    fn handle(&mut self, msg: ProxyMessage, ctx: &mut Self::Context) -> Self::Result {
        match msg {
            ProxyMessage::WebSocketMessage(text) => {
                // Forward text message to client
                ctx.text(text);
            },
            ProxyMessage::WebSocketBinary(data) => {
                // Forward binary message to client
                ctx.binary(data);
            },
            ProxyMessage::WebSocketPing => {
                // Forward ping to client
                ctx.ping(b"");
            },
            ProxyMessage::WebSocketPong => {
                // Nothing to do
            },
            ProxyMessage::WebSocketClose => {
                // Close client connection
                ctx.close(None);
            },
            ProxyMessage::Disconnected => {
                tracing::warn!("WebSocket server connection lost for client: {}", self.client_id);
                
                // Clear sink
                self.ws_sink = None;
                self.is_connected_to_server = false;
                
                // Attempt reconnection
                let backoff = std::cmp::min(
                    2u64.pow(self.reconnect_attempts),
                    60 // Cap at 60 seconds
                );
                
                ctx.run_later(Duration::from_secs(backoff), |act, ctx| {
                    tracing::info!("Attempting reconnection for client: {}", act.client_id);
                    act.connect_to_ws_server(ctx);
                });
                
                // Increment reconnect counter
                self.reconnect_attempts += 1;
            }
        }
    }
}

// Configure proxy routes - updated for session validation
pub fn configure(cfg: &mut web::ServiceConfig) {
    // Create shared state for active connections
    let active_connections = web::Data::new(ActiveConnections::new());
    
    // Register the active connections data
    cfg.app_data(active_connections.clone());
    
    // Configure WebSocket route
    cfg.service(
        web::resource("/ws/{client_id}")
            .route(web::get().to(ws_route))
    );
}

// WebSocket route handler - updated for session validation
async fn ws_route(
    req: HttpRequest,
    stream: web::Payload,
    path: web::Path<(String,)>,
    config: web::Data<Config>,
    active_connections: web::Data<ActiveConnections>,
    registry: web::Data<Addr<ClientRegistryActor>>,
) -> Result<HttpResponse, Error> {
    // Extract client_id from path
    let client_id_str = &path.0;
    let client_id = match Uuid::parse_str(client_id_str) {
        Ok(id) => id,
        Err(_) => return Ok(HttpResponse::BadRequest().finish()),
    };
    
    // Get session token from cookie
    let session_token = req.cookie("sploots_session").map(|c| c.value().to_string());
    
    // Validate session if token is present
    if let Some(token) = &session_token {
        match registry.send(GetClientSession { session_token: token.clone() }).await {
            Ok(SessionResult::Success(session)) => {
                // Check if client ID matches session
                if session.client_id != client_id {
                    tracing::warn!(
                        "Client ID mismatch: requested {}, session has {}", 
                        client_id, session.client_id
                    );
                    return Ok(HttpResponse::Forbidden().finish());
                }
                
                // Check if another connection exists for this session
                if let Some(existing_conn) = active_connections.connections.get(token) {
                    tracing::info!(
                        "Existing connection found for session {}, closing it", 
                        session.client_id
                    );
                    
                    // Send close message to existing connection
                    // This is a policy choice: last connection wins
                    let _ = existing_conn.send(ProxyMessage::WebSocketClose).await;
                }
                
                tracing::info!("Session validated for client: {}", client_id);
            },
            Ok(SessionResult::Expired) => {
                tracing::warn!("Expired session token for client: {}", client_id);
                return Ok(HttpResponse::Unauthorized().finish());
            },
            Ok(_) => {
                tracing::warn!("Invalid session token for client: {}", client_id);
                return Ok(HttpResponse::Unauthorized().finish());
            },
            Err(e) => {
                tracing::error!("Error validating session: {}", e);
                return Ok(HttpResponse::InternalServerError().finish());
            }
        }
    } else {
        tracing::warn!("No session token provided for client: {}", client_id);
        // We could reject unauthenticated connections here, but for now we'll allow them
        // This is a policy choice and can be changed
    }
    
    // Get WebSocket server URL from config
    let ws_server_url = format!("ws://{}", config.websocket_server_addr);
    
    // Create proxy actor with all dependencies injected 
    let proxy = ProxyActor::new(
        client_id, 
        ws_server_url, 
        session_token,
        Some(registry.get_ref().clone()),
        Some(active_connections.clone())
    );
    
    // Start WebSocket connection
    ws::start(proxy, &req, stream)
}