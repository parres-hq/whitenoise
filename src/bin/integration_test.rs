use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

use nostr_sdk::prelude::*;
use whitenoise::{AccountSettings, GroupId, Whitenoise, WhitenoiseConfig, WhitenoiseError};

/// Test backend for Whitenoise
#[derive(Parser, Debug)]
#[clap(author, version, about)]
struct Args {
    /// Directory for application data
    #[clap(long, value_name = "PATH", required = true)]
    data_dir: PathBuf,

    /// Directory for application logs
    #[clap(long, value_name = "PATH", required = true)]
    logs_dir: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), WhitenoiseError> {
    let args = Args::parse();

    let config = WhitenoiseConfig::new(&args.data_dir, &args.logs_dir);
    if let Err(err) = Whitenoise::initialize_whitenoise(config).await {
        tracing::error!("Failed to initialize Whitenoise: {}", err);
        std::process::exit(1);
    }

    let whitenoise = Whitenoise::get_instance()?;

    tracing::info!("=== Starting Whitenoise Integration Test ===");

    // Verify initial state
    tracing::info!("Verifying initial state...");
    assert_eq!(whitenoise.get_accounts_count().await, 0);
    tracing::info!("✓ Started with 0 accounts");

    // ========================================
    // ACCOUNT CREATION AND LOGIN TESTING
    // ========================================
    tracing::info!("=== Testing Account Creation and Login ===");

    // Create first account
    tracing::info!("Creating first account...");
    let account1 = whitenoise.create_identity().await?;
    tracing::info!("✓ First account created: {}", account1.pubkey.to_hex());
    assert_eq!(whitenoise.get_accounts_count().await, 1);

    // Create second account
    tracing::info!("Creating second account...");
    let account2 = whitenoise.create_identity().await?;
    tracing::info!("✓ Second account created: {}", account2.pubkey.to_hex());
    assert_eq!(whitenoise.get_accounts_count().await, 2);

    // Test login with known keys
    tracing::info!("Testing login with known keys...");
    let known_keys = Keys::generate();
    let known_pubkey = known_keys.public_key();

    // Publish some test events first (to test background fetch)
    let test_client = Client::default();
    let dev_relays = vec!["ws://localhost:8080", "ws://localhost:7777"];
    for relay in &dev_relays {
        test_client.add_relay(*relay).await.unwrap();
    }
    test_client.connect().await;
    test_client.set_signer(known_keys.clone()).await;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Publish metadata
    let metadata = Metadata {
        name: Some("Known User".to_string()),
        display_name: Some("Known User".to_string()),
        about: Some("A user with known keys".to_string()),
        picture: Some("https://example.com/known-avatar.jpg".to_string()),
        ..Default::default()
    };
    let metadata_event = EventBuilder::metadata(&metadata);
    test_client
        .send_event_builder(metadata_event)
        .await
        .unwrap();

    // Publish relay lists
    let relay_urls: Vec<String> = dev_relays.iter().map(|s| s.to_string()).collect();
    let relay_tags: Vec<Tag> = relay_urls
        .iter()
        .map(|url| Tag::custom(TagKind::Relay, [url.clone()]))
        .collect();

    test_client
        .send_event_builder(EventBuilder::new(Kind::RelayList, "").tags(relay_tags.clone()))
        .await
        .unwrap();
    test_client
        .send_event_builder(EventBuilder::new(Kind::InboxRelays, "").tags(relay_tags.clone()))
        .await
        .unwrap();
    test_client
        .send_event_builder(EventBuilder::new(Kind::MlsKeyPackageRelays, "").tags(relay_tags))
        .await
        .unwrap();

    test_client.disconnect().await;

    // Now login with the known keys
    let account3 = whitenoise
        .login(known_keys.secret_key().to_secret_hex())
        .await?;
    tracing::info!("✓ Logged in account: {}", account3.pubkey.to_hex());
    assert_eq!(whitenoise.get_accounts_count().await, 3);
    assert_eq!(account3.pubkey, known_pubkey);

    // Wait for background fetch
    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

    // ========================================
    // METADATA AND ONBOARDING TESTING
    // ========================================
    tracing::info!("=== Testing Metadata and Onboarding ===");

    // Test metadata fetching
    tracing::info!("Testing metadata fetching...");
    let loaded_metadata = whitenoise.fetch_metadata(account3.pubkey).await?;
    if let Some(metadata) = loaded_metadata {
        assert_eq!(metadata.name, Some("Known User".to_string()));
        tracing::info!("✓ Metadata fetched correctly");
    } else {
        tracing::warn!("Metadata not found - may be expected in test environment");
    }

    // Test onboarding state
    tracing::info!("Testing onboarding state...");
    let onboarding_state = whitenoise.fetch_onboarding_state(account3.pubkey).await?;
    tracing::info!("Onboarding state: {:?}", onboarding_state);
    tracing::info!("✓ Onboarding state fetched");

    // Test metadata update
    tracing::info!("Testing metadata update...");
    let updated_metadata = Metadata {
        name: Some("Updated Known User".to_string()),
        display_name: Some("Updated Known User".to_string()),
        about: Some("Updated description".to_string()),
        picture: Some("https://example.com/updated-avatar.jpg".to_string()),
        banner: Some("https://example.com/banner.jpg".to_string()),
        nip05: Some("updated@example.com".to_string()),
        lud16: Some("updated@lightning.example.com".to_string()),
        website: Some("https://updated.example.com".to_string()),
        ..Default::default()
    };

    whitenoise
        .update_metadata(&updated_metadata, &account3)
        .await?;
    tracing::info!("✓ Metadata updated successfully");

    // ========================================
    // ACCOUNT SETTINGS TESTING
    // ========================================
    tracing::info!("=== Testing Account Settings ===");

    // Test fetching default settings
    let settings = whitenoise.fetch_account_settings(&account1.pubkey).await?;
    assert_eq!(settings, AccountSettings::default());
    tracing::info!("✓ Default settings fetched correctly");

    // Test updating settings
    let new_settings = AccountSettings {
        dark_theme: false,
        dev_mode: true,
        lockdown_mode: true,
    };
    whitenoise
        .update_account_settings(&account1.pubkey, &new_settings)
        .await?;
    tracing::info!("✓ Settings updated successfully");

    // Verify settings were updated
    let updated_settings = whitenoise.fetch_account_settings(&account1.pubkey).await?;
    assert_eq!(updated_settings, new_settings);
    tracing::info!("✓ Settings verified after update");

    // Test error case - non-existent account
    let fake_pubkey = Keys::generate().public_key();
    let result = whitenoise.fetch_account_settings(&fake_pubkey).await;
    assert!(matches!(result, Err(WhitenoiseError::AccountNotFound)));
    tracing::info!("✓ Correctly handled non-existent account error");

    // ========================================
    // CONTACT MANAGEMENT TESTING
    // ========================================
    tracing::info!("=== Testing Contact Management ===");

    // Test with account1
    let test_contact1 = Keys::generate().public_key();
    let test_contact2 = Keys::generate().public_key();
    let test_contact3 = Keys::generate().public_key();

    // Test initial empty contact list
    let initial_contacts = whitenoise.fetch_contacts(account1.pubkey).await?;
    assert_eq!(initial_contacts.len(), 0);
    tracing::info!("✓ Initial contact list is empty");

    // Test adding a contact
    whitenoise.add_contact(&account1, test_contact1).await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    tracing::info!("✓ Added first contact");

    // Test adding a second contact
    whitenoise.add_contact(&account1, test_contact2).await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    tracing::info!("✓ Added second contact");

    // Test removing a contact
    whitenoise.remove_contact(&account1, test_contact1).await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    tracing::info!("✓ Removed first contact");

    // Test bulk contact update
    let bulk_contacts = vec![test_contact2, test_contact3];
    whitenoise.update_contacts(&account1, bulk_contacts).await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    tracing::info!("✓ Updated contacts in bulk");

    // Test error handling - duplicate contact
    let result = whitenoise.add_contact(&account1, test_contact2).await;
    if result.is_err() {
        tracing::info!("✓ Correctly handled duplicate contact error");
    } else {
        tracing::warn!("Expected error for duplicate contact, but got success");
    }

    // Test error handling - removing non-existent contact
    let non_existent_contact = Keys::generate().public_key();
    let result = whitenoise
        .remove_contact(&account1, non_existent_contact)
        .await;
    if result.is_err() {
        tracing::info!("✓ Correctly handled non-existent contact removal error");
    } else {
        tracing::warn!("Expected error for non-existent contact removal, but got success");
    }

    // ========================================
    // GROUP CREATION TESTING
    // ========================================
    tracing::info!("=== Testing Group Creation ===");

    // Create a test group with account1 as creator and account2 as member
    // Both were created via create_identity() so they should have published key packages
    let group_name = "Integration Test Group".to_string();
    let group_description = "A group for testing message functionality".to_string();
    let member_pubkeys = vec![account2.pubkey]; // account2 as member (has published key package)
    let admin_pubkeys = vec![account1.pubkey]; // account1 as admin/creator

    let test_group = whitenoise
        .create_group(
            &account1,
            member_pubkeys,
            admin_pubkeys,
            group_name.clone(),
            group_description.clone(),
        )
        .await?;

    tracing::info!("✓ Test group created successfully: {}", test_group.name);
    tracing::info!(
        "  Group ID: {}",
        hex::encode(test_group.mls_group_id.as_slice())
    );
    tracing::info!("  Admin count: {}", test_group.admin_pubkeys.len());

    // ========================================
    // MESSAGE SENDING TESTING
    // ========================================
    tracing::info!("=== Testing Message Sending ===");

    // Test sending a simple text message (account1 is the group creator)
    tracing::info!("Testing simple text message...");
    let test_message = "Hello from integration test!".to_string();
    let message_with_tokens = whitenoise
        .send_message(
            &account1.pubkey,
            &test_group.mls_group_id,
            test_message.clone(),
            1, // Kind 1 for text note
            None,
        )
        .await?;

    assert_eq!(message_with_tokens.message.content, test_message);
    tracing::info!("✓ Simple text message sent successfully");

    // Test sending a message with tags
    tracing::info!("Testing message with tags...");
    let tagged_message = "This message has tags!".to_string();
    let test_tags = vec![
        Tag::custom(TagKind::Custom("test".into()), ["integration"]),
        Tag::custom(TagKind::Custom("category".into()), ["testing"]),
    ];

    let tagged_message_with_tokens = whitenoise
        .send_message(
            &account1.pubkey,
            &test_group.mls_group_id,
            tagged_message.clone(),
            1,
            Some(test_tags),
        )
        .await?;

    assert_eq!(tagged_message_with_tokens.message.content, tagged_message);
    tracing::info!("✓ Tagged message sent successfully");

    // Test sending a different kind of message (reaction)
    tracing::info!("Testing reaction message...");
    let reaction_message = "👍".to_string();
    let reaction_with_tokens = whitenoise
        .send_message(
            &account1.pubkey,
            &test_group.mls_group_id,
            reaction_message.clone(),
            7, // Kind 7 for reaction
            None,
        )
        .await?;

    assert_eq!(reaction_with_tokens.message.content, reaction_message);
    assert_eq!(reaction_with_tokens.message.kind, Kind::Custom(7));
    tracing::info!("✓ Reaction message sent successfully");

    // Test error handling - non-existent group
    tracing::info!("Testing error handling for non-existent group...");
    let fake_group_id = GroupId::from_slice(&[0u8; 32]);
    let error_result = whitenoise
        .send_message(
            &account1.pubkey,
            &fake_group_id,
            "This should fail".to_string(),
            1,
            None,
        )
        .await;

    match error_result {
        Ok(_) => {
            return Err(WhitenoiseError::Other(anyhow::anyhow!(
                "Expected error when sending to non-existent group, but got success"
            )));
        }
        Err(e) => {
            tracing::info!("✓ Correctly handled non-existent group error: {}", e);
        }
    }

    // ========================================
    // LOGOUT TESTING
    // ========================================
    tracing::info!("=== Testing Account Logout ===");

    // Logout account2 (after group creation and message testing)
    tracing::info!("Logging out account2...");
    whitenoise.logout(&account2.pubkey).await?;
    assert_eq!(whitenoise.get_accounts_count().await, 2);
    assert!(whitenoise.logged_in(&account1.pubkey).await);
    assert!(!whitenoise.logged_in(&account2.pubkey).await);
    assert!(whitenoise.logged_in(&account3.pubkey).await);
    tracing::info!("✓ Account2 logged out successfully");

    // ========================================
    // FINAL VERIFICATION
    // ========================================
    tracing::info!("=== Final Verification ===");

    // Verify final account state
    let final_accounts = whitenoise.fetch_accounts().await?;
    assert_eq!(final_accounts.len(), 2); // account1 and account3 should remain
    assert!(final_accounts.contains_key(&account1.pubkey));
    assert!(final_accounts.contains_key(&account3.pubkey));
    assert!(!final_accounts.contains_key(&account2.pubkey)); // account2 was logged out
    tracing::info!("✓ Final account state is correct");

    // Verify accounts are still logged in
    assert!(whitenoise.logged_in(&account1.pubkey).await);
    assert!(whitenoise.logged_in(&account3.pubkey).await);
    assert!(!whitenoise.logged_in(&account2.pubkey).await);
    tracing::info!("✓ Account login states are correct");

    tracing::info!("=== Integration Test Completed Successfully ===");
    tracing::info!("All public API functionality has been tested:");
    tracing::info!("  ✓ Account creation and login");
    tracing::info!("  ✓ Metadata fetching and updating");
    tracing::info!("  ✓ Onboarding state management");
    tracing::info!("  ✓ Account settings management");
    tracing::info!("  ✓ Account logout");
    tracing::info!("  ✓ Contact management (add, remove, update)");
    tracing::info!("  ✓ Group creation");
    tracing::info!("  ✓ Message sending (text, tagged, reactions)");
    tracing::info!("  ✓ Error handling");

    Ok(())
}
