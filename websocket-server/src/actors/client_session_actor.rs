// websocket-server/src/actors/client_session_actor.rs
use actix::{Actor, ActorContext, AsyncContext, StreamHandler, Addr, Context, Handler};
use actix_web_actors::ws;
use common::{ClientMessage, SystemMessage}; // Assuming you might need SystemMessage later
use uuid::Uuid;
use std::time::{Duration, Instant, SystemTime}; // Added SystemTime
use super::state_manager::{
    StateManagerActor, UnregisterClient, ConnectionState,
    UpdateClientState, ClientActivity
};
use super::router_actor::{ClientActorMessage, RouterActor}; // Import RouterActor

// Enhanced client session actor
pub struct ClientSessionActor {
    client_id: Uuid,
    authenticated: bool,
    wallet_address: Option<String>,
    state_manager: Option<Addr<StateManagerActor>>,
    router: Option<Addr<RouterActor>>, // <-- Add Router Actor Address
    last_heartbeat: Instant,
    heartbeat_interval: Duration,
    heartbeat_timeout: Duration,
    message_buffer: Vec<String>,
}

impl ClientSessionActor {
    pub fn new(client_id: Uuid) -> Self {
        Self {
            client_id,
            authenticated: false,
            wallet_address: None,
            state_manager: None,
            router: None, // <-- Initialize router as None
            last_heartbeat: Instant::now(),
            heartbeat_interval: Duration::from_secs(5),
            heartbeat_timeout: Duration::from_secs(30),
            message_buffer: Vec::new(),
        }
    }

    // (Optional) Keep with_auth if needed, update it too
    pub fn with_auth(client_id: Uuid, wallet_address: String) -> Self {
        Self {
            client_id,
            authenticated: true,
            wallet_address: Some(wallet_address),
            state_manager: None,
            router: None, // <-- Initialize router as None
            last_heartbeat: Instant::now(),
            heartbeat_interval: Duration::from_secs(5),
            heartbeat_timeout: Duration::from_secs(30),
            message_buffer: Vec::new(),
        }
    }

    // Set state manager reference
    pub fn set_state_manager(&mut self, addr: Addr<StateManagerActor>) {
        self.state_manager = Some(addr);
    }

    // Set router reference <-- Add this method
    pub fn set_router(&mut self, addr: Addr<RouterActor>) {
        self.router = Some(addr);
    }

    // Enhanced heartbeat (no changes needed here for routing)
    fn heartbeat(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(self.heartbeat_interval, |act, ctx| {
            if Instant::now().duration_since(act.last_heartbeat) > act.heartbeat_timeout {
                tracing::warn!("Client heartbeat timeout: {}", act.client_id);

                if let Some(state_manager) = &act.state_manager {
                    state_manager.do_send(UpdateClientState {
                        client_id: act.client_id,
                        state: ConnectionState::Disconnected,
                        last_seen_update: true,
                    });
                    state_manager.do_send(UnregisterClient {
                        client_id: act.client_id,
                    });
                }
                ctx.stop();
                return;
            }
            ctx.ping(b"");
        });
    }

    // (No changes needed for buffer_message or send_buffered_messages)
    pub fn buffer_message(&mut self, message: String) {
        if self.message_buffer.len() < 100 {
            self.message_buffer.push(message);
        } else {
            tracing::warn!("Message buffer full for client: {}, dropping message", self.client_id);
        }
    }

    fn send_buffered_messages(&mut self, ctx: &mut ws::WebsocketContext<Self>) {
        if !self.message_buffer.is_empty() {
            tracing::info!("Sending {} buffered messages for client: {}",
                          self.message_buffer.len(), self.client_id);
            for msg in self.message_buffer.drain(..) {
                ctx.text(msg);
            }
        }
    }


    // Update activity with state manager (no changes needed here)
    fn update_activity(&self, is_message: bool) {
        if let Some(state_manager) = &self.state_manager {
            state_manager.do_send(ClientActivity {
                client_id: self.client_id,
                is_message,
            });
        }
    }
}

impl Actor for ClientSessionActor {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!("Client connected: {}", self.client_id);
        self.last_heartbeat = Instant::now();
        self.heartbeat(ctx);
        self.send_buffered_messages(ctx);
        // Notify state manager (no changes needed here)
        if let Some(state_manager) = &self.state_manager {
             state_manager.do_send(UpdateClientState {
                 client_id: self.client_id,
                 state: ConnectionState::Connected, // Assume connected on start
                 last_seen_update: true,
             });
        }
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        tracing::info!("Client disconnected: {}", self.client_id);
        if let Some(state_manager) = &self.state_manager {
             state_manager.do_send(UpdateClientState {
                 client_id: self.client_id,
                 state: ConnectionState::Disconnected,
                 last_seen_update: true,
             });
             state_manager.do_send(UnregisterClient {
                 client_id: self.client_id,
             });
        }
         // Also unregister from router if router exists
         if let Some(router) = &self.router {
             router.do_send(super::router_actor::UnregisterClient {
                 client_id: self.client_id,
             });
         }
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for ClientSessionActor {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(msg)) => {
                self.last_heartbeat = Instant::now();
                self.update_activity(false);
                ctx.pong(&msg);
            },
            Ok(ws::Message::Pong(_)) => {
                self.last_heartbeat = Instant::now();
                self.update_activity(false);
            },
            Ok(ws::Message::Text(text)) => {
                self.last_heartbeat = Instant::now();
                self.update_activity(true);
                tracing::debug!("Received raw message from client {}: {}", self.client_id, text);

                // ---- START ROUTING LOGIC ----
                let client_msg = ClientMessage {
                    client_id: self.client_id,
                    content: text.to_string(),
                    authenticated: self.authenticated,
                    wallet_address: self.wallet_address.clone(),
                    timestamp: SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                };

                if let Some(router) = &self.router {
                    tracing::info!("Forwarding message from client {} to router", self.client_id);
                    // Send the parsed ClientMessage to the RouterActor
                    if let Err(e) = router.try_send(client_msg) {
                         tracing::error!("Failed to send message to router: {}", e);
                         // Optionally inform the client about the internal error
                         ctx.text("Error: Could not process message internally.");
                    }
                } else {
                    tracing::error!("Router address not available for client {}", self.client_id);
                    // Echo back with error if no router is configured (shouldn't happen with proper setup)
                     ctx.text("Error: Internal routing not configured.");
                }
                // ---- END ROUTING LOGIC ----

            },
            Ok(ws::Message::Binary(_)) => {
                self.last_heartbeat = Instant::now();
                self.update_activity(true);
                tracing::warn!("Binary messages not supported for client: {}", self.client_id);
                ctx.text("Binary messages not supported");
            },
            Ok(ws::Message::Close(reason)) => {
                tracing::info!("Client closing connection: {:?}", reason);
                if let Some(state_manager) = &self.state_manager {
                     state_manager.do_send(UpdateClientState {
                         client_id: self.client_id,
                         state: ConnectionState::Disconnected,
                         last_seen_update: true,
                     });
                 }
                ctx.close(reason); // This will trigger stopped()
            },
            Ok(ws::Message::Continuation(_)) => {
                self.last_heartbeat = Instant::now();
                tracing::trace!("Received continuation frame from client {}", self.client_id);
            },
            Ok(ws::Message::Nop) => {
                self.last_heartbeat = Instant::now();
                tracing::trace!("Received nop frame from client {}", self.client_id);
            },
            Err(e) => {
                tracing::error!("WebSocket protocol error from client {}: {}", self.client_id, e);
                if let Some(state_manager) = &self.state_manager {
                     state_manager.do_send(UpdateClientState {
                         client_id: self.client_id,
                         state: ConnectionState::Error,
                         last_seen_update: true,
                     });
                 }
                 ctx.stop(); // Stop actor on protocol error
            }
        }
    }
}

// Handle messages FROM the router TO this client
impl Handler<ClientActorMessage> for ClientSessionActor {
    type Result = ();

    fn handle(&mut self, msg: ClientActorMessage, ctx: &mut Self::Context) -> Self::Result {
        tracing::info!("Received message via router for client {}, sending to WebSocket", self.client_id);
        ctx.text(msg.content);
    }
}