// websocket-server/src/actors/agent_actor.rs
use actix::{Actor, AsyncContext, ActorContext, StreamHandler, Context, Addr, Handler};
use actix_web_actors::ws;
use common::{AgentMessage, SystemMessage}; // Assuming SystemMessage might be used
use std::time::{Duration, Instant, SystemTime}; // Added SystemTime
use uuid::Uuid; // Added Uuid (might be needed if AgentMessage uses it)
use super::state_manager::{
    StateManagerActor, UnregisterAgent, ConnectionState,
    UpdateAgentState, AgentActivity
};
use super::router_actor::{AgentActorMessage, RouterActor}; // Import RouterActor

// Enhanced agent actor
pub struct AgentActor {
    id: String,
    token: String,
    state_manager: Option<Addr<StateManagerActor>>,
    router: Option<Addr<RouterActor>>, // <-- Add Router Actor Address
    last_heartbeat: Instant,
    heartbeat_interval: Duration,
    heartbeat_timeout: Duration,
    reconnect_attempts: u32,
    message_buffer: Vec<AgentMessage>, // Changed buffer to AgentMessage if needed
}

impl AgentActor {
    pub fn new(id: String, token: String) -> Self {
        Self {
            id,
            token,
            state_manager: None,
            router: None, // <-- Initialize router as None
            last_heartbeat: Instant::now(),
            heartbeat_interval: Duration::from_secs(5),
            heartbeat_timeout: Duration::from_secs(30),
            reconnect_attempts: 0,
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
                tracing::warn!("Agent heartbeat timeout: {}", act.id);

                if let Some(state_manager) = &act.state_manager {
                    state_manager.do_send(UpdateAgentState {
                        agent_id: act.id.clone(),
                        state: ConnectionState::Reconnecting, // Mark as reconnecting on timeout
                        last_seen_update: true,
                    });
                }

                // Calculate backoff duration
                let backoff_seconds = std::cmp::min(
                    2u64.pow(act.reconnect_attempts),
                    60
                );
                let backoff = Duration::from_secs(backoff_seconds);

                tracing::info!(
                    "Agent {} reconnection attempt {} scheduled in {} seconds",
                    act.id, act.reconnect_attempts + 1, backoff_seconds
                );

                // Attempt reconnection with backoff
                 ctx.run_later(backoff, |act, ctx| {
                     act.last_heartbeat = Instant::now(); // Reset for attempt
                     act.reconnect_attempts += 1;
                     // Just send a ping to check connectivity
                     ctx.ping(b"reconnect_attempt");
                 });

                 // Don't stop the actor immediately, let reconnect logic run
            } else {
                // Send regular ping
                ctx.ping(b"");
            }
        });
    }


    // Buffer a message (if needed)
    pub fn buffer_message(&mut self, message: AgentMessage) {
        if self.message_buffer.len() < 100 {
            self.message_buffer.push(message);
        } else {
            tracing::warn!("Message buffer full for agent: {}, dropping message", self.id);
        }
    }

    // Send buffered messages
    fn send_buffered_messages(&mut self, ctx: &mut ws::WebsocketContext<Self>) {
        if !self.message_buffer.is_empty() {
            tracing::info!("Sending {} buffered messages for agent: {}",
                          self.message_buffer.len(), self.id);
            for msg in self.message_buffer.drain(..) {
                if let Ok(serialized) = serde_json::to_string(&msg) {
                    ctx.text(serialized);
                } else {
                    tracing::error!("Failed to serialize buffered AgentMessage for agent {}", self.id);
                }
            }
        }
    }

    // Update activity with state manager (no changes needed here)
    fn update_activity(&self, is_message: bool) {
        if let Some(state_manager) = &self.state_manager {
            state_manager.do_send(AgentActivity {
                agent_id: self.id.clone(),
                is_message,
            });
        }
    }
}

impl Actor for AgentActor {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!("Agent connected: {}", self.id);
        self.last_heartbeat = Instant::now();
        self.reconnect_attempts = 0; // Reset on successful connection
        self.heartbeat(ctx);
        self.send_buffered_messages(ctx);
        // Notify state manager
        if let Some(state_manager) = &self.state_manager {
            state_manager.do_send(UpdateAgentState {
                agent_id: self.id.clone(),
                state: ConnectionState::Connected,
                last_seen_update: true,
            });
        }
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        tracing::info!("Agent disconnected: {}", self.id);
        if let Some(state_manager) = &self.state_manager {
             state_manager.do_send(UpdateAgentState {
                 agent_id: self.id.clone(),
                 state: ConnectionState::Disconnected,
                 last_seen_update: true,
             });
             state_manager.do_send(UnregisterAgent {
                 agent_id: self.id.clone(),
             });
         }
         // Also unregister from router if router exists
         if let Some(router) = &self.router {
             router.do_send(super::router_actor::UnregisterAgent {
                 agent_id: self.id.clone(),
             });
         }
    }
}

// Handle messages FROM the router TO this agent
impl Handler<AgentActorMessage> for AgentActor {
    type Result = ();

    fn handle(&mut self, msg: AgentActorMessage, ctx: &mut Self::Context) -> Self::Result {
        tracing::info!("Received message via router for agent {}, sending to WebSocket", self.id);
        // Update last heartbeat? Maybe not on outgoing messages unless needed.
        // self.last_heartbeat = Instant::now();
        // self.update_activity(true); // Indicate outgoing activity?
        ctx.text(msg.content);
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for AgentActor {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(msg)) => {
                self.last_heartbeat = Instant::now();
                self.update_activity(false);
                ctx.pong(&msg);
                 // Reset reconnect attempts on successful ping
                 if self.reconnect_attempts > 0 {
                     tracing::info!("Agent {} reconnected successfully via ping", self.id);
                     self.reconnect_attempts = 0;
                     if let Some(state_manager) = &self.state_manager {
                         state_manager.do_send(UpdateAgentState {
                             agent_id: self.id.clone(),
                             state: ConnectionState::Connected,
                             last_seen_update: true,
                         });
                     }
                 }
            },
            Ok(ws::Message::Pong(_)) => {
                self.last_heartbeat = Instant::now();
                self.update_activity(false);
                 // Reset reconnect attempts on successful pong (could be response to reconnection ping)
                 if self.reconnect_attempts > 0 {
                     tracing::info!("Agent {} reconnected successfully via pong", self.id);
                     self.reconnect_attempts = 0;
                     if let Some(state_manager) = &self.state_manager {
                         state_manager.do_send(UpdateAgentState {
                             agent_id: self.id.clone(),
                             state: ConnectionState::Connected,
                             last_seen_update: true,
                         });
                     }
                 }
            },
            Ok(ws::Message::Text(text)) => {
                self.last_heartbeat = Instant::now();
                self.update_activity(true);
                tracing::debug!("Received raw message from agent {}: {}", self.id, text);

                // ---- START ROUTING LOGIC ----
                match serde_json::from_str::<AgentMessage>(&text) {
                    Ok(agent_msg) => {
                        if let Some(router) = &self.router {
                             tracing::info!("Forwarding message from agent {} to router", self.id);
                             // Send the parsed AgentMessage to the RouterActor
                             if let Err(e) = router.try_send(agent_msg) {
                                 tracing::error!("Failed to send agent message to router: {}", e);
                                 // Optionally inform the agent about the internal error
                                 // Note: Need a way to structure error messages back to agent
                             }
                        } else {
                             tracing::error!("Router address not available for agent {}", self.id);
                        }
                    },
                    Err(e) => {
                         tracing::warn!("Failed to parse message from agent {}: {}", self.id, e);
                         // Optionally inform agent about the error
                         // Note: Need a structured error message format
                         let error_response = AgentMessage {
                            target_client_id: None, // Or maybe specific client if context known?
                            content: format!("Error: Received malformed message - {}", e),
                            timestamp: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs(),
                         };
                         if let Ok(json) = serde_json::to_string(&error_response) {
                            // This echoes back, maybe not ideal, consider logging only or specific error channel
                            // ctx.text(json);
                            tracing::error!("Sent parsing error back to agent {}", self.id); // Log instead of echoing?
                         }
                    }
                }
                 // ---- END ROUTING LOGIC ----
            },
            Ok(ws::Message::Binary(_)) => {
                self.last_heartbeat = Instant::now();
                self.update_activity(true);
                tracing::warn!("Binary messages not supported for agent: {}", self.id);
            },
            Ok(ws::Message::Close(reason)) => {
                tracing::info!("Agent closing connection: {:?}", reason);
                if let Some(state_manager) = &self.state_manager {
                     state_manager.do_send(UpdateAgentState {
                         agent_id: self.id.clone(),
                         state: ConnectionState::Disconnected,
                         last_seen_update: true,
                     });
                 }
                 ctx.close(reason); // Triggers stopped()
            },
            Ok(ws::Message::Continuation(_)) => {
                self.last_heartbeat = Instant::now();
                tracing::trace!("Received continuation frame from agent {}", self.id);
            },
            Ok(ws::Message::Nop) => {
                self.last_heartbeat = Instant::now();
                tracing::trace!("Received nop frame from agent {}", self.id);
            },
            Err(e) => {
                tracing::error!("WebSocket protocol error for agent {}: {}", self.id, e);
                if let Some(state_manager) = &self.state_manager {
                    state_manager.do_send(UpdateAgentState {
                        agent_id: self.id.clone(),
                        state: ConnectionState::Error,
                        last_seen_update: true,
                    });
                 }
                 ctx.stop(); // Stop actor on protocol error
            }
        }
    }
}