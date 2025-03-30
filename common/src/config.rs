// Common Crate - config.rs
// my-actix-system/common/src/config.rs

/// Central configuration for both services

#[derive(Clone)]
pub struct Config {
    pub websocket_server_addr: String,
    pub web_server_addr: String,
    pub agent_token: String,  // Pre-shared key for agent authentication
}

impl Default for Config {
    fn default() -> Self {
        Self {
            websocket_server_addr: "127.0.0.1:8080".to_string(),
            web_server_addr: "127.0.0.1:8081".to_string(),
            agent_token: "dev_token".to_string(),
        }
    }
}

impl Config {
    pub fn from_env() -> Self {
        let websocket_server_addr = std::env::var("WEBSOCKET_SERVER_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:8080".to_string());
            
        let web_server_addr = std::env::var("WEB_SERVER_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:8081".to_string());
            
        let agent_token = std::env::var("AGENT_TOKEN")
            .unwrap_or_else(|_| "dev_token".to_string());
            
        Self {
            websocket_server_addr,
            web_server_addr,
            agent_token,
        }
    }
}
