// web-server/src/api/sessions.rs
use actix::Addr;
use actix_web::{get, post, delete, web, HttpRequest, HttpResponse, Responder, cookie::{Cookie, SameSite}};
use actix_web::cookie::time::Duration as CookieDuration;
use common::models::session::{ClientSessionResponse, SessionResult};
use serde_json::json;
use uuid::Uuid;
use serde::{Deserialize, Serialize};
use jsonwebtoken::errors::Error as JwtError;
use crate::client_registry::{
    ClientRegistryActor, 
    RegisterAnonymousClient, 
    GetClientSession,
    GetClientSessionById,
    InvalidateClientSession,
    UpdateClientSession
};

// Cookie name for session tracking
const SESSION_COOKIE_NAME: &str = "sploots_session";
// Cookie max age in seconds (24 hours)
const COOKIE_MAX_AGE: i64 = 86400;
// Add JWT secret to configuration
// This should be loaded from environment or config file
const JWT_SECRET: &[u8] = b"your_jwt_secret_key_here";
// ****************************
// In web-server/src/main.rs or config.rs: 
// pub fn get_jwt_secret() -> Vec<u8> {
//     std::env::var("JWT_SECRET")
//         .unwrap_or_else(|_| "insecure_default_only_for_development".to_string())
//         .into_bytes()
// }

// // Then in sessions.rs
// let jwt_secret = get_jwt_secret();
// *****************************



// Request structure for session upgrade
#[derive(Deserialize)]
pub struct UpgradeRequest {
    pub wallet_address: String,
}

// Response structure for successful upgrade
#[derive(Serialize)]
pub struct UpgradeResponse {
    pub status: String,
    pub token: String,
}

#[get("/")]
pub async fn api_index() -> impl Responder {
    HttpResponse::Ok().json(json!({
        "name": "Agent Bridge Platform API",
        "version": "0.1.0"
    }))
}

// Create a new client session or return existing one
#[post("/client")]
pub async fn create_client(
    req: HttpRequest,
    registry: web::Data<Addr<ClientRegistryActor>>,
) -> impl Responder {
    // Check for existing session cookie
    if let Some(cookie) = req.cookie(SESSION_COOKIE_NAME) {
        let session_token = cookie.value().to_string();
        
        // Attempt to retrieve existing session
        match registry.send(GetClientSession { session_token }).await {
            Ok(SessionResult::Success(session)) => {
                // Found valid session, return client info
                let mut response = ClientSessionResponse::from(&session);
                response.new_session = false;
                
                tracing::info!("Returning existing client session: {}", session.client_id);
                return HttpResponse::Ok().json(response);
            },
            Ok(SessionResult::Expired) => {
                tracing::info!("Session expired, creating new client");
                // Fall through to create new session
            },
            Ok(_) => {
                tracing::info!("Session not found or invalid, creating new client");
                // Fall through to create new session
            },
            Err(e) => {
                tracing::error!("Error retrieving session: {}", e);
                return HttpResponse::InternalServerError().json(json!({
                    "error": "Internal server error"
                }));
            }
        }
    }
    
    // Create new anonymous client
    match registry.send(RegisterAnonymousClient).await {
        Ok((client_id, session_token)) => {
            // Create session cookie
            let cookie = Cookie::build(SESSION_COOKIE_NAME, session_token)
                .path("/")
                .secure(true)
                .http_only(true)
                .same_site(SameSite::Strict)
                .max_age(CookieDuration::seconds(COOKIE_MAX_AGE))
                .finish();
            
            // Create response
            let response = json!({
                "client_id": client_id,
                "created_at": chrono::Utc::now(),
                "is_authenticated": false,
                "wallet_address": null,
                "new_session": true
            });
            
            tracing::info!("Created new client session: {}", client_id);
            
            // Return response with cookie
            HttpResponse::Ok()
                .cookie(cookie)
                .json(response)
        },
        Err(e) => {
            tracing::error!("Error creating client: {}", e);
            HttpResponse::InternalServerError().json(json!({
                "error": "Internal server error"
            }))
        }
    }
}

// Get client session information
#[get("/client/{client_id}")]
pub async fn get_client_info(
    path: web::Path<(String,)>,
    req: HttpRequest,
    registry: web::Data<Addr<ClientRegistryActor>>,
) -> impl Responder {
    let client_id_str = &path.0;
    
    // Parse client ID
    let client_id = match Uuid::parse_str(client_id_str) {
        Ok(id) => id,
        Err(_) => {
            return HttpResponse::BadRequest().json(json!({
                "error": "Invalid client ID format"
            }));
        }
    };
    
    // Check for session cookie
    if let Some(cookie) = req.cookie(SESSION_COOKIE_NAME) {
        let session_token = cookie.value().to_string();
        
        // Retrieve session
        match registry.send(GetClientSession { session_token }).await {
            Ok(SessionResult::Success(session)) => {
                // Verify client ID matches the session
                if session.client_id != client_id {
                    tracing::warn!(
                        "Client ID mismatch: requested {}, session has {}", 
                        client_id, session.client_id
                    );
                    return HttpResponse::Forbidden().json(json!({
                        "error": "Access denied"
                    }));
                }
                
                // Return client info
                let response = ClientSessionResponse::from(&session);
                return HttpResponse::Ok().json(response);
            },
            Ok(SessionResult::Expired) => {
                return HttpResponse::Unauthorized().json(json!({
                    "error": "Session expired"
                }));
            },
            Ok(_) => {
                return HttpResponse::Unauthorized().json(json!({
                    "error": "Invalid session"
                }));
            },
            Err(e) => {
                tracing::error!("Error retrieving session: {}", e);
                return HttpResponse::InternalServerError().json(json!({
                    "error": "Internal server error"
                }));
            }
        }
    }
    
    // Try to get session by client ID as fallback
    match registry.send(GetClientSessionById { client_id }).await {
        Ok(SessionResult::Success(session)) => {
            // Session found, but cookie is missing - this is unusual
            tracing::warn!("Session found for client {} but cookie is missing", client_id);
            
            // Create new cookie
            let cookie = Cookie::build(SESSION_COOKIE_NAME, session.session_token.clone())
                .path("/")
                .secure(true)
                .http_only(true)
                .same_site(SameSite::Strict)
                .max_age(CookieDuration::seconds(COOKIE_MAX_AGE))
                .finish();
            
            // Return client info with cookie
            let response = ClientSessionResponse::from(&session);
            HttpResponse::Ok()
                .cookie(cookie)
                .json(response)
        },
        Ok(_) => {
            // No session found
            HttpResponse::NotFound().json(json!({
                "error": "Client not found"
            }))
        },
        Err(e) => {
            tracing::error!("Error retrieving session by client ID: {}", e);
            HttpResponse::InternalServerError().json(json!({
                "error": "Internal server error"
            }))
        }
    }
}

// Invalidate/logout client session
#[delete("/client/session")]
pub async fn invalidate_session(
    req: HttpRequest,
    registry: web::Data<Addr<ClientRegistryActor>>,
) -> impl Responder {
    // Check for session cookie
    if let Some(cookie) = req.cookie(SESSION_COOKIE_NAME) {
        let session_token = cookie.value().to_string();
        
        // Attempt to invalidate session
        match registry.send(InvalidateClientSession { session_token }).await {
            Ok(true) => {
                // Session invalidated successfully
                // Create empty cookie to clear the session
                let cookie = Cookie::build(SESSION_COOKIE_NAME, "")
                    .path("/")
                    .max_age(CookieDuration::seconds(0))
                    .finish();
                
                tracing::info!("Session invalidated successfully");
                
                // Return success with cookie clearing
                HttpResponse::Ok()
                    .cookie(cookie)
                    .json(json!({
                        "status": "success",
                        "message": "Session invalidated"
                    }))
            },
            Ok(false) => {
                // Session not found
                tracing::info!("Attempt to invalidate non-existent session");
                HttpResponse::NotFound().json(json!({
                    "error": "Session not found"
                }))
            },
            Err(e) => {
                tracing::error!("Error invalidating session: {}", e);
                HttpResponse::InternalServerError().json(json!({
                    "error": "Internal server error"
                }))
            }
        }
    } else {
        // No session cookie found
        HttpResponse::BadRequest().json(json!({
            "error": "No session cookie found"
        }))
    }
}

// Session upgrade endpoint
#[post("/sessions/upgrade")]
pub async fn upgrade_session(
    req: HttpRequest,
    data: web::Json<UpgradeRequest>,
    registry: web::Data<Addr<ClientRegistryActor>>,
) -> impl Responder {
    // 1. Extract client ID from existing session cookie
    if let Some(cookie) = req.cookie("sploots_session") {
        let session_token = cookie.value().to_string();
        
        // 2. Upgrade session with wallet address n  NEED TO CHECK THIS UPDATE AS IT SEESM THAT IT IS NOT CORRECTLY UTILIZING THE ALREADY DEFINED TYPES 
        match registry.send(UpdateClientSession {
            session_token,
            is_authenticated: Some(true),
            wallet_address: Some(Some(data.wallet_address.clone())),
            metadata: None,
            extend_ttl: true,
        }).await {
            Ok(SessionResult::Success(mut session)) => {
                // 3. Generate JWT for WebSocket auth
                match session.generate_auth_token(JWT_SECRET) {
                    Ok(token) => {
                        // 4. Return token
                        return HttpResponse::Ok().json(UpgradeResponse {
                            status: "success".to_string(),
                            token,
                        });
                    },
                    Err(_) => {
                        return HttpResponse::InternalServerError().json(json!({
                            "error": "Failed to generate authentication token"
                        }));
                    }
                }
            },
            Ok(SessionResult::Expired) => {
                return HttpResponse::Unauthorized().json(json!({
                    "error": "Session expired"
                }));
            },
            Ok(_) => {
                return HttpResponse::Unauthorized().json(json!({
                    "error": "Invalid session"
                }));
            },
            Err(e) => {
                tracing::error!("Error upgrading session: {}", e);
                return HttpResponse::InternalServerError().json(json!({
                    "error": "Internal server error"
                }));
            }
        }
    }
    
    HttpResponse::Unauthorized().json(json!({
        "error": "No session cookie found"
    }))
}

// Add to web-server/src/api/sessions.rs

// JWT validation middleware
fn validate_jwt(req: &HttpRequest) -> Result<(Uuid, String), HttpResponse> {
    if let Some(auth_header) = req.headers().get("Authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            if auth_str.starts_with("Bearer ") {
                let token = &auth_str[7..]; // Skip "Bearer "
                match common::utils::validate_jwt_token(token, JWT_SECRET) {
                    Ok((client_id, wallet_address)) => {
                        return Ok((client_id, wallet_address));
                    },
                    Err(e) => {
                        tracing::warn!("JWT validation failed: {}", e);
                        return Err(HttpResponse::Unauthorized().json(json!({
                            "error": "Invalid token"
                        })));
                    }
                }
            }
        }
    }
    
    Err(HttpResponse::Unauthorized().json(json!({
        "error": "Authorization header missing or invalid"
    })))
}

// Test endpoint for JWT validation
#[get("/protected")]
pub async fn protected_endpoint(
    req: HttpRequest,
) -> impl Responder {
    match validate_jwt(&req) {
        Ok((client_id, wallet_address)) => {
            HttpResponse::Ok().json(json!({
                "status": "success",
                "message": "Authenticated access granted",
                "client_id": client_id,
                "wallet_address": wallet_address
            }))
        },
        Err(response) => response
    }
}