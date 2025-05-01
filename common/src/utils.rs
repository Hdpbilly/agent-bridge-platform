// Common Crate - utils.rs 
// my-actix-system/common/src/utils.rs
use tracing::Level;
use tracing_subscriber::FmtSubscriber;
use jsonwebtoken::{encode, decode, Header, Algorithm, Validation, EncodingKey, DecodingKey};
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use std::time::{SystemTime, UNIX_EPOCH};

/// Setup tracing for consistent logging across services
pub fn setup_tracing() {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    
    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set tracing subscriber");
}

// JWT Claims structure
#[derive(Debug, Serialize, Deserialize)]
pub struct JwtClaims {
    pub sub: String,       // client_id
    pub wallet: String,    // wallet_address 
    pub exp: usize,        // expiration time
    pub iat: usize,        // issued at time
}

// Generate JWT token from client_id and wallet_address
pub fn generate_jwt_token(client_id: &Uuid, wallet_address: &str, secret: &[u8]) -> Result<String, jsonwebtoken::errors::Error> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs() as usize;
    
    let claims = JwtClaims {
        sub: client_id.to_string(),
        wallet: wallet_address.to_string(),
        iat: now,
        exp: now + 86400, // 24 hours expiration
    };
    
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret)
    )
}

// Validate JWT token and extract client_id and wallet_address
pub fn validate_jwt_token(token: &str, secret: &[u8]) -> Result<(Uuid, String), jsonwebtoken::errors::Error> {
    let validation = Validation::new(Algorithm::HS256);
    
    let token_data = decode::<JwtClaims>(
        token,
        &DecodingKey::from_secret(secret),
        &validation
    )?;
    
    let uuid = Uuid::parse_str(&token_data.claims.sub)
        .map_err(|_| jsonwebtoken::errors::ErrorKind::InvalidSubject)?;
    
    Ok((uuid, token_data.claims.wallet))
}