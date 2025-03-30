// web-server/src/proxy.rs
use actix::{Actor, StreamHandler, AsyncContext, Context, ActorContext, Addr, Message, Handler};
use actix_web::{web, HttpRequest, HttpResponse, Error};
use actix_web_actors::ws;
use serde::{Serialize, Deserialize};
use tokio::sync::mpsc;
use futures::{StreamExt, SinkExt};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message as WsMessage};
use tokio_tungstenite::tungstenite::protocol::frame::CloseFrame;
use common::Config;
use tokio_tungstenite::tungstenite::error::Error as WsError;
use uuid::Uuid;
use std::time::{Duration, Instant};
use std::borrow::Cow;
use std::convert::TryFrom;
use tungstenite::protocol::frame::coding::CloseCode as TungsteniteCloseCode;

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

// Enhanced ProxyActor with real proxying
pub struct ProxyActor {
    client_id: Uuid,
    ws_sink: Option<mpsc::Sender<WsMessage>>,
    last_heartbeat: Instant,
    reconnect_attempts: u32,
    ws_server_url: String,
}

impl ProxyActor {
    pub fn new(client_id: Uuid, ws_server_url: String) -> Self {
        Self { 
            client_id,
            ws_sink: None,
            last_heartbeat: Instant::now(),
            reconnect_attempts: 0,
            ws_server_url,
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
}

impl Actor for ProxyActor {
    type Context = ws::WebsocketContext<Self>;
    
    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!("Proxy started for client: {}", self.client_id);
        
        // Setup heartbeat
        self.heartbeat(ctx);
        
        // Connect to WebSocket server
        self.connect_to_ws_server(ctx);
    }
    
    fn stopped(&mut self, _ctx: &mut Self::Context) {
        tracing::info!("Proxy stopped for client: {}", self.client_id);
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
            // Ok(ws::Message::Frame(_)) => {
            //     // Low-level frame, typically not handled directly
            // },
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

// Configure proxy routes - updated for Phase 2
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::resource("/ws/{client_id}")
            .route(web::get().to(ws_route))
    );
}

// WebSocket route handler - updated for Phase 2
async fn ws_route(
    req: HttpRequest,
    stream: web::Payload,
    path: web::Path<(String,)>,
    config: web::Data<Config>,
) -> Result<HttpResponse, Error> {
    // Extract client_id from path
    let client_id_str = &path.0;
    let client_id = match Uuid::parse_str(client_id_str) {
        Ok(id) => id,
        Err(_) => return Ok(HttpResponse::BadRequest().finish()),
    };
    
    // Get WebSocket server URL from config
    let ws_server_url = format!("ws://{}", config.websocket_server_addr);
    
    // Create proxy actor with WebSocket server URL
    let proxy = ProxyActor::new(client_id, ws_server_url);
    
    // Start WebSocket connection
    ws::start(proxy, &req, stream)
}