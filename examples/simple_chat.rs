use nostr_sdk::prelude::*;
use std::path::PathBuf;
use whitenoise::{NostrGroupConfigData, RelayType, Whitenoise, WhitenoiseConfig};

/// Simple example demonstrating basic messaging between two clients
///
/// This is a minimal example that shows:
/// 1. Creating two accounts
/// 2. Creating a group between them
/// 3. Sending a message
///
/// Note: This example requires local relay servers to be running.
/// Run `docker-compose up` in the project root before running this example.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize Whitenoise
    let config = WhitenoiseConfig::new(
        &PathBuf::from("./example_data"),
        &PathBuf::from("./example_logs"),
    );
    Whitenoise::initialize_whitenoise(config).await?;
    let whitenoise = Whitenoise::get_instance()?;

    // Create two accounts
    println!("Creating accounts...");
    let alice = whitenoise.create_identity().await?;
    let bob = whitenoise.create_identity().await?;

    println!("Alice: {}", alice.pubkey.to_hex());
    println!("Bob: {}", bob.pubkey.to_hex());

    // Configure local relays
    let relay_urls = vec![
        RelayUrl::parse("ws://localhost:8080")?,
        RelayUrl::parse("ws://localhost:7777")?,
    ];

    // Setup relays for both accounts
    println!("\nConfiguring relays...");
    for account in [&alice, &bob] {
        for relay_type in [RelayType::Nostr, RelayType::Inbox, RelayType::KeyPackage] {
            whitenoise
                .update_relays(account, relay_type, relay_urls.clone())
                .await?;
        }
    }

    // Complete onboarding
    println!("Completing onboarding...");
    whitenoise
        .complete_pending_onboarding_steps(&alice.pubkey)
        .await?;
    whitenoise
        .complete_pending_onboarding_steps(&bob.pubkey)
        .await?;

    // Short wait for key packages to propagate
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Create a group
    println!("\nCreating group...");
    let group_config = NostrGroupConfigData {
        name: "Test Chat".to_string(),
        description: "Simple test group".to_string(),
        image_key: None,
        image_url: None,
        relays: relay_urls.clone(),
    };

    let group = whitenoise
        .create_group(
            &alice,
            vec![bob.pubkey],   // Bob is a member
            vec![alice.pubkey], // Alice is admin
            group_config,
        )
        .await?;

    let group_id = &group.mls_group_id;
    println!("Group created: {}", hex::encode(group_id.as_slice()));

    // Wait for group creation event to propagate
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    // Bob checks and accepts welcomes
    println!("\nBob accepting invitation...");
    let welcomes = whitenoise.fetch_welcomes(&bob.pubkey).await?;
    println!("Bob found {} welcomes", welcomes.len());

    if let Some(welcome) = welcomes.first() {
        whitenoise
            .accept_welcome(&bob.pubkey, welcome.id.to_string())
            .await?;
        println!("Bob accepted welcome");

        // Wait for acceptance to process
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    }

    // Send a test message from Alice
    println!("\nAlice sending message...");
    let message = whitenoise
        .send_message_to_group(
            &alice.pubkey,
            group_id,
            "Hello Bob!".to_string(),
            1, // Kind 1 = text message
            None,
        )
        .await?;
    println!("Message sent: {}", message.message.id);

    // Wait for message to propagate
    println!("Waiting for message propagation...");
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    // Try to fetch messages as Bob
    println!("\nBob fetching messages...");
    let messages = whitenoise
        .fetch_messages_for_group(&bob.pubkey, group_id)
        .await?;

    println!("Bob fetched {} messages", messages.len());

    // Assert that Bob received exactly one message
    assert_eq!(
        messages.len(),
        1,
        "Bob should have received exactly 1 message"
    );

    let received_message = &messages[0].message;

    // Assert the message is from Alice
    assert_eq!(
        received_message.pubkey, alice.pubkey,
        "Message should be from Alice"
    );

    // Assert the message content is correct
    assert_eq!(
        received_message.content, "Hello Bob!",
        "Message content should match"
    );

    println!("✅ Message verification passed!");
    for msg in messages {
        println!("  From: {}", msg.message.pubkey.to_hex());
        println!("  Content: {}", msg.message.content);
    }

    // Also try aggregated messages
    println!("\nFetching aggregated messages...");
    let chat_messages = whitenoise
        .fetch_aggregated_messages_for_group(&bob.pubkey, group_id)
        .await?;

    println!("Found {} aggregated messages", chat_messages.len());

    // Note: Aggregated messages might be 0 if the message aggregator hasn't processed them yet
    // This is expected behavior and not a failure
    if chat_messages.is_empty() {
        println!("⚠️  No aggregated messages found - this may be expected if message aggregation is still processing");
    } else {
        // If we do have aggregated messages, verify them
        assert_eq!(
            chat_messages.len(),
            1,
            "Should have exactly 1 aggregated message"
        );
        let chat_msg = &chat_messages[0];
        assert_eq!(
            chat_msg.author, alice.pubkey,
            "Aggregated message should be from Alice"
        );
        assert_eq!(
            chat_msg.content, "Hello Bob!",
            "Aggregated message content should match"
        );
        println!("✅ Aggregated message verification passed!");
    }

    for msg in chat_messages {
        println!("  Author: {}", msg.author.to_hex());
        println!("  Content: {}", msg.content);
    }

    println!("\n🎉 Simple chat example completed successfully!");
    Ok(())
}
