use nostr_sdk::prelude::*;
use std::path::PathBuf;
use whitenoise::{NostrGroupConfigData, RelayType, Whitenoise, WhitenoiseConfig};

// Helper struct for displaying messages
#[derive(Debug)]
struct ChatMessageDisplay {
    author: PublicKey,
    content: String,
}

/// Example demonstrating two clients connecting and exchanging messages
///
/// This example shows:
/// 1. Creating two separate client accounts
/// 2. Setting up relays for communication
/// 3. Creating a group chat between them
/// 4. Sending messages back and forth
/// 5. Fetching and displaying received messages
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

    // Create two new identities (accounts) using login approach
    println!("🔑 Creating two accounts...");
    let alice_keys = Keys::generate();
    let bob_keys = Keys::generate();

    let alice = whitenoise
        .login(alice_keys.secret_key().to_secret_hex())
        .await?;
    let bob = whitenoise
        .login(bob_keys.secret_key().to_secret_hex())
        .await?;

    println!("👤 Alice: {}", alice.pubkey.to_hex());
    println!("👤 Bob: {}", bob.pubkey.to_hex());

    // Configure relay URLs (using local test relays - make sure Docker is running)
    let relay_urls = vec![
        RelayUrl::parse("ws://localhost:8080")?,
        RelayUrl::parse("ws://localhost:7777")?,
    ];

    // Update relays for both accounts
    println!("\n📡 Configuring relays for both accounts...");
    for account in [&alice, &bob] {
        whitenoise
            .update_relays(account, RelayType::Nostr, relay_urls.clone())
            .await?;
        whitenoise
            .update_relays(account, RelayType::Inbox, relay_urls.clone())
            .await?;
        whitenoise
            .update_relays(account, RelayType::KeyPackage, relay_urls.clone())
            .await?;
    }

    // Complete onboarding steps (publish key packages)
    println!("\n🔧 Completing onboarding for both accounts...");
    whitenoise
        .complete_pending_onboarding_steps(&alice.pubkey)
        .await?;
    whitenoise
        .complete_pending_onboarding_steps(&bob.pubkey)
        .await?;

    // Wait a bit for key packages to propagate
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Create a group chat between Alice and Bob
    println!("\n👥 Creating group chat...");
    let group_config = NostrGroupConfigData {
        name: "Alice and Bob Chat".to_string(),
        description: "Example private chat between two clients".to_string(),
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
    println!("✅ Group created: {}", hex::encode(group_id.as_slice()));

    // Wait for group creation to propagate
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Bob needs to accept the welcome/invitation to join the group
    println!("\n📨 Bob checking for invitations...");
    let welcomes = whitenoise.fetch_welcomes(&bob.pubkey).await?;
    if let Some(welcome) = welcomes.first() {
        println!("✅ Bob found welcome invitation, accepting...");
        whitenoise
            .accept_welcome(&bob.pubkey, welcome.id.to_string())
            .await?;

        // Wait for Bob to join the group
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // Bob fetches his groups to ensure he's properly synced
        let bob_groups = whitenoise.fetch_groups(&bob, false).await?;
        println!("   Bob is now in {} groups", bob_groups.len());
    }

    // Alice sends a message to the group
    println!("\n💬 Alice sending message...");
    let alice_message = whitenoise
        .send_message_to_group(
            &alice.pubkey,
            group_id,
            "Hello Bob! This is Alice. How are you?".to_string(),
            1, // Kind 1 = text message
            None,
        )
        .await?;
    println!("✅ Alice sent: {:?}", alice_message);

    // Wait for message to propagate
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    // Bob fetches messages from the group
    println!("\n📥 Bob fetching messages...");
    let bob_raw_messages = whitenoise
        .fetch_messages_for_group(&bob.pubkey, group_id)
        .await?;

    // Convert to a simpler format for display
    let bob_messages: Vec<_> = bob_raw_messages
        .iter()
        .map(|msg_with_tokens| ChatMessageDisplay {
            author: msg_with_tokens.message.pubkey,
            content: msg_with_tokens.message.content.clone(),
        })
        .collect();

    println!("   Bob sees {} messages", bob_messages.len());

    // Assert Bob received Alice's message
    assert!(
        !bob_messages.is_empty(),
        "Bob should have received at least one message"
    );

    let alice_message_found = bob_messages
        .iter()
        .any(|msg| msg.author == alice.pubkey && msg.content.contains("Hello Bob! This is Alice"));
    assert!(
        alice_message_found,
        "Bob should have received Alice's greeting message"
    );

    for msg in &bob_messages {
        println!(
            "   Bob received: {} - \"{}\"",
            if msg.author == alice.pubkey {
                "Alice"
            } else {
                "Unknown"
            },
            msg.content
        );
    }

    // Bob sends a reply
    println!("\n💬 Bob sending reply...");
    let bob_message = whitenoise
        .send_message_to_group(
            &bob.pubkey,
            group_id,
            "Hi Alice! I'm doing great, thanks for asking!".to_string(),
            1,
            None,
        )
        .await?;
    println!("✅ Bob sent: {:?}", bob_message);

    // Wait for message to propagate
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    // Alice fetches all messages
    println!("\n📥 Alice fetching all messages...");
    let alice_raw_messages = whitenoise
        .fetch_messages_for_group(&alice.pubkey, group_id)
        .await?;

    let alice_messages: Vec<_> = alice_raw_messages
        .iter()
        .map(|msg_with_tokens| ChatMessageDisplay {
            author: msg_with_tokens.message.pubkey,
            content: msg_with_tokens.message.content.clone(),
        })
        .collect();

    println!(
        "📜 Complete conversation history ({} messages):",
        alice_messages.len()
    );

    // Assert Alice can see both messages
    assert!(
        alice_messages.len() >= 2,
        "Alice should see at least 2 messages (her own + Bob's reply)"
    );

    let alice_sent_found = alice_messages
        .iter()
        .any(|msg| msg.author == alice.pubkey && msg.content.contains("Hello Bob! This is Alice"));
    let bob_reply_found = alice_messages
        .iter()
        .any(|msg| msg.author == bob.pubkey && msg.content.contains("Hi Alice! I'm doing great"));

    assert!(alice_sent_found, "Alice should see her own message");
    assert!(bob_reply_found, "Alice should see Bob's reply");

    for msg in &alice_messages {
        let sender = if msg.author == alice.pubkey {
            "Alice"
        } else if msg.author == bob.pubkey {
            "Bob"
        } else {
            "Unknown"
        };
        println!("   {} - \"{}\"", sender, msg.content);
    }

    // Demonstrate sending a few more messages
    println!("\n🔄 Continuing conversation...");

    // Alice sends another message
    whitenoise
        .send_message_to_group(
            &alice.pubkey,
            group_id,
            "Would you like to grab coffee sometime?".to_string(),
            1,
            None,
        )
        .await?;

    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // Bob replies
    whitenoise
        .send_message_to_group(
            &bob.pubkey,
            group_id,
            "Sure! How about tomorrow at 3pm?".to_string(),
            1,
            None,
        )
        .await?;

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Fetch final conversation
    println!("\n📜 Final conversation:");
    let final_raw_messages = whitenoise
        .fetch_messages_for_group(&alice.pubkey, group_id)
        .await?;

    let final_messages: Vec<_> = final_raw_messages
        .iter()
        .map(|msg_with_tokens| ChatMessageDisplay {
            author: msg_with_tokens.message.pubkey,
            content: msg_with_tokens.message.content.clone(),
        })
        .collect();

    println!("Total messages: {}", final_messages.len());

    // Final assertion: should have all 4 messages
    assert!(
        final_messages.len() >= 4,
        "Should have at least 4 messages in final conversation"
    );

    // Verify specific messages exist
    let coffee_message = final_messages
        .iter()
        .any(|msg| msg.author == alice.pubkey && msg.content.contains("coffee"));
    let three_pm_message = final_messages
        .iter()
        .any(|msg| msg.author == bob.pubkey && msg.content.contains("3pm"));

    assert!(coffee_message, "Should find Alice's coffee message");
    assert!(three_pm_message, "Should find Bob's 3pm response");

    for msg in &final_messages {
        let sender = if msg.author == alice.pubkey {
            "Alice"
        } else if msg.author == bob.pubkey {
            "Bob"
        } else {
            "Unknown"
        };
        println!("   {} - \"{}\"", sender, msg.content);
    }

    println!("\n🎉 Two-client chat example completed successfully!");
    println!("✅ All message assertions passed - messages were successfully exchanged!");
    println!("Note: In a real application, you would handle messages asynchronously");
    println!("and update the UI as new messages arrive.");

    Ok(())
}
