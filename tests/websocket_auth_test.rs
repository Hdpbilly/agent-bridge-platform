// tests/websocket_auth_test.rs
use futures_util::{SinkExt, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
struct UpgradeResponse {
    status: String,
    token: String,
}

#[tokio::test]
async fn test_websocket_jwt_auth() -> Result<(), Box<dyn std::error::Error>> {
    let http_client = Client::new();
    let base_url = "http://localhost:8081";
    
    // 1. Create anonymous session and get client_id
    let create_resp = http_client.post(&format!("{}/api/client", base_url))
        .send()
        .await?;
    
    let cookie = create_resp.headers()
        .get(reqwest::header::SET_COOKIE)
        .expect("No session cookie found")
        .to_str()?
        .to_string();
    
    #[derive(Deserialize)]
    struct ClientInfo {
        client_id: Uuid,
    }
    
    let client_info: ClientInfo = create_resp.json().await?;
    
    // 2. Upgrade session with wallet address
    let wallet_address = "0x71C7656EC7ab88b098defB751B7401B5f6d8976F";
    
    let upgrade_resp = http_client.post(&format!("{}/api/sessions/upgrade", base_url))
        .header(reqwest::header::COOKIE, &cookie)
        .json(&json!({
            "wallet_address": wallet_address
        }))
        .send()
        .await?;
    
    let upgrade_data: UpgradeResponse = upgrade_resp.json().await?;
    println!("Token: {}", upgrade_data.token);
    
    // 3. Connect to WebSocket with JWT
    let ws_url = format!("ws://localhost:8081/ws/{}", client_info.client_id);
    
    // Add JWT to the headers
    let mut request = http::Request::builder()
        .uri(ws_url)
        .header("Cookie", &cookie)
        .header("Authorization", format!("Bearer {}", upgrade_data.token))
        .body(())?;
    
    let (ws_stream, _) = connect_async(request).await?;
    let (mut write, mut read) = ws_stream.split();
    
    // 4. Send a test message
    write.send(Message::Text("Test message from authenticated client".into())).await?;
    
    // 5. Verify we get a response (not disconnected due to auth failure)
    if let Some(msg) = read.next().await {
        let msg = msg?;
        println!("Received response: {:?}", msg);
        // At minimum, we've verified the connection wasn't rejected
    }
    
    Ok(())
}