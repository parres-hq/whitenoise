use std::time::{Duration, Instant};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures_util::{SinkExt, StreamExt};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let relays = vec![
        "ws://localhost:8080",
        "ws://localhost:7777",
    ];
    
    let mut all_healthy = true;
    
    for relay_url in relays {
        println!("Testing Nostr relay: {}", relay_url);
        
        match test_nostr_relay(relay_url).await {
            Ok(_) => println!("✓ {} is healthy", relay_url),
            Err(e) => {
                println!("✗ {} failed health check: {}", relay_url, e);
                all_healthy = false;
            }
        }
    }
    
    // Test blossom server
    println!("Testing Blossom server: http://localhost:3000");
    match test_http_endpoint("http://localhost:3000").await {
        Ok(_) => println!("✓ Blossom server is healthy"),
        Err(e) => {
            println!("✗ Blossom server failed: {}", e);
            all_healthy = false;
        }
    }
    
    if all_healthy {
        println!("All services are healthy!");
        Ok(())
    } else {
        eprintln!("Some services failed health checks");
        std::process::exit(1);
    }
}

async fn test_nostr_relay(url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let timeout = Duration::from_secs(10);
    let start = Instant::now();
    
    // Connect to WebSocket
    let (ws_stream, _) = tokio::time::timeout(timeout, connect_async(url)).await??;
    let (mut write, mut read) = ws_stream.split();
    
    // Send a REQ message to test relay functionality
    let req_msg = json!([
        "REQ",
        "health_check_sub",
        {
            "kinds": [0],
            "limit": 1
        }
    ]);
    
    write.send(Message::Text(req_msg.to_string())).await?;
    
    // Wait for any response (EOSE, EVENT, or NOTICE)
    while start.elapsed() < timeout {
        match tokio::time::timeout(Duration::from_millis(100), read.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                // Parse response
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Some(array) = parsed.as_array() {
                        if !array.is_empty() {
                            let msg_type = array[0].as_str().unwrap_or("");
                            match msg_type {
                                "EOSE" | "EVENT" | "NOTICE" => {
                                    // Send CLOSE to clean up
                                    let close_msg = json!(["CLOSE", "health_check_sub"]);
                                    let _ = write.send(Message::Text(close_msg.to_string())).await;
                                    return Ok(());
                                }
                                _ => continue,
                            }
                        }
                    }
                }
            }
            Ok(Some(Ok(Message::Close(_)))) => {
                return Err("WebSocket closed unexpectedly".into());
            }
            Ok(Some(Ok(Message::Binary(_)))) => {
                // Ignore binary messages
                continue;
            }
            Ok(Some(Ok(Message::Ping(_)))) => {
                // Ignore ping messages
                continue;
            }
            Ok(Some(Ok(Message::Pong(_)))) => {
                // Ignore pong messages
                continue;
            }
            Ok(Some(Ok(Message::Frame(_)))) => {
                // Ignore frame messages
                continue;
            }
            Ok(Some(Err(e))) => {
                return Err(Box::new(e));
            }
            Ok(None) => {
                return Err("WebSocket stream ended".into());
            }
            Err(_) => {
                // Timeout on this iteration, continue
                continue;
            }
        }
    }
    
    Err("Timeout waiting for relay response".into())
}

async fn test_http_endpoint(url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let response = tokio::time::timeout(
        Duration::from_secs(5),
        client.get(url).send()
    ).await??;
    
    if response.status().is_success() || response.status().as_u16() == 404 {
        // 404 is okay for blossom server root endpoint
        Ok(())
    } else {
        Err(format!("HTTP {} from {}", response.status(), url).into())
    }
}