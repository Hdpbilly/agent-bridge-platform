// tests/jwt_auth_test.rs
use reqwest::{Client, header};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
struct ClientResponse {
    client_id: Uuid,
    is_authenticated: bool,
    wallet_address: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpgradeResponse {
    status: String,
    token: String,
}

#[tokio::test]
async fn test_jwt_authentication() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();
    let base_url = "http://localhost:8081";
    
    // 1. Create anonymous session
    let create_resp = client.post(&format!("{}/api/client", base_url))
        .send()
        .await?;
    
    // Get the session cookie
    let cookie = create_resp.headers()
        .get(header::SET_COOKIE)
        .expect("No session cookie found")
        .to_str()?
        .to_string();
    
    let client_data: ClientResponse = create_resp.json().await?;
    println!("Created client: {:?}", client_data);
    
    // 2. Upgrade session with wallet address
    let wallet_address = "0x71C7656EC7ab88b098defB751B7401B5f6d8976F"; // Example Ethereum address
    
    let upgrade_resp = client.post(&format!("{}/api/sessions/upgrade", base_url))
        .header(header::COOKIE, &cookie)
        .json(&json!({
            "wallet_address": wallet_address
        }))
        .send()
        .await?;
    
    let upgrade_data: UpgradeResponse = upgrade_resp.json().await?;
    println!("Upgrade response: {:?}", upgrade_data);
    assert_eq!(upgrade_data.status, "success");
    assert!(!upgrade_data.token.is_empty(), "JWT token should not be empty");
    
    // 3. Test JWT authentication with a protected endpoint
    // For example, the WebSocket endpoint or a special test endpoint
    
    // First, let's verify the client is now authenticated
    let auth_check = client.get(&format!("{}/api/client/{}", base_url, client_data.client_id))
        .header(header::COOKIE, &cookie)
        .send()
        .await?;
    
    let auth_data: ClientResponse = auth_check.json().await?;
    println!("Authenticated client: {:?}", auth_data);
    assert!(auth_data.is_authenticated, "Client should be authenticated");
    assert_eq!(auth_data.wallet_address.as_deref(), Some(wallet_address));
    
    // 4. Try to access a protected resource using the JWT
    // This could be a WebSocket connection or a special endpoint for testing
    
    // For demonstration, we'll create a simple endpoint that validates JWTs
    // Note: You'll need to add this endpoint to your API
    
    let protected_resp = client.get(&format!("{}/api/protected", base_url))
        .header("Authorization", format!("Bearer {}", upgrade_data.token))
        .send()
        .await?;
    
    assert!(protected_resp.status().is_success(), 
           "Protected endpoint should accept the JWT token");
    
    Ok(())
}