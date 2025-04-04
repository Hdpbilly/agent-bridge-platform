// web-server/src/utils/token.rs
use rand::{thread_rng, Rng};
use rand::distributions::Alphanumeric;
use sha2::{Sha256, Digest};
use std::time::{SystemTime, UNIX_EPOCH};

/// Generate a cryptographically secure random token of specified length
pub fn generate_secure_token(length: usize) -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(length)
        .map(char::from)
        .collect()
}

/// Generate a nonce with timestamp and random component
pub fn generate_nonce() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    
    let random_part = generate_secure_token(16);
    format!("{}-{}", timestamp, random_part)
}

/// Hash a string using SHA-256
pub fn hash_string(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();
    format!("{:x}", result)
}

/// Create a session token with more entropy
pub fn create_session_token() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    
    let random_part = generate_secure_token(32);
    let input = format!("{}-{}", timestamp, random_part);
    
    // Hash for additional security
    hash_string(&input)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_generate_secure_token() {
        let token = generate_secure_token(32);
        assert_eq!(token.len(), 32);
    }
    
    #[test]
    fn test_generate_nonce() {
        let nonce = generate_nonce();
        assert!(nonce.contains('-'));
        let parts: Vec<&str> = nonce.split('-').collect();
        assert_eq!(parts.len(), 2);
    }
    
    #[test]
    fn test_hash_string() {
        let input = "test string";
        let hash = hash_string(input);
        assert_eq!(hash.len(), 64); // SHA-256 produces 64 hex characters
    }
    
    #[test]
    fn test_create_session_token() {
        let token = create_session_token();
        assert_eq!(token.len(), 64); // SHA-256 produces 64 hex characters
        
        // Tokens should be unique
        let token2 = create_session_token();
        assert_ne!(token, token2);
    }
}