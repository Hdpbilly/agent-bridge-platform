// Web Server - proxy.rs
// my-actix-system/web-server/src/proxy.rs
use actix_web::{web, HttpRequest, HttpResponse, Error};
use actix_web_actors::ws;
use actix::{Actor, StreamHandler, AsyncContext};
use uuid::Uuid;

/// Basic WebSocket proxy actor for Phase 1
/// This will connect to the websocket-server and proxy messages
pub struct ProxyActor {
    client_id: Uuid,
}

impl ProxyActor {
    pub fn new(client_id: Uuid) -> Self {
        Self { client_id }
    }
}

impl Actor for ProxyActor {
    type Context = ws::WebsocketContext<Self>;
    
    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!("Proxy started for client: {}", self.client_id);
        
        // Setup heartbeat
        self.heartbeat(ctx);
        
        // For Phase 1, we're just echoing messages
        // Phase 2 will implement actual proxying to websocket-server
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for ProxyActor {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(msg)) => ctx.pong(&msg),
            Ok(ws::Message::Text(text)) => {
                // Echo for Phase 1
                tracing::info!("Proxy received from client {}: {}", self.client_id, text);
                ctx.text(text);
            },
            _ => (),
        }
    }
}

impl ProxyActor {
    fn heartbeat(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(std::time::Duration::from_secs(30), |_act, ctx| {
            ctx.ping(b"");
        });
    }
}

/// Configure proxy routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::resource("/ws/{client_id}")
            .route(web::get().to(ws_route))
    );
}

/// WebSocket route for clients
async fn ws_route(
    req: HttpRequest,
    stream: web::Payload,
    path: web::Path<(String,)>,
) -> Result<HttpResponse, Error> {
    // Extract client_id from path
    let client_id_str = &path.0;
    let client_id = match Uuid::parse_str(client_id_str) {
        Ok(id) => id,
        Err(_) => return Ok(HttpResponse::BadRequest().finish()),
    };
    
    // Create proxy actor
    let proxy = ProxyActor::new(client_id);
    
    // Start WebSocket connection
    ws::start(proxy, &req, stream)
}