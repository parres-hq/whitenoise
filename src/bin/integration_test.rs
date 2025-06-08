use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

use nostr_sdk::prelude::*;
use whitenoise::{Whitenoise, WhitenoiseConfig, WhitenoiseError};

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
    let mut whitenoise: Whitenoise = match Whitenoise::initialize_whitenoise(config).await {
        Ok(whitenoise) => whitenoise,
        Err(err) => {
            tracing::error!("Failed to initialize Whitenoise: {}", err);
            std::process::exit(1);
        }
    };

    tracing::info!("=== Testing basic account creation and management ===");

    tracing::debug!("Whitenoise state after initialization: {:?}", whitenoise);
    assert_eq!(whitenoise.accounts.len(), 0);
    assert_eq!(whitenoise.active_account, None);

    tracing::info!("Creating first account...");
    let created_account = whitenoise.create_identity().await?;
    tracing::debug!("Created account: {:?}", created_account);
    tracing::debug!(
        "Whitenoise state after creating first account: {:?}",
        whitenoise
    );

    assert_eq!(whitenoise.accounts.len(), 1);
    assert_eq!(whitenoise.active_account, Some(created_account.pubkey));
    tracing::info!("First account created and set as active");

    tracing::info!("Creating second account...");
    let created_account_2 = whitenoise.create_identity().await?;
    tracing::debug!("Created account 2: {:?}", created_account_2);
    tracing::debug!(
        "Whitenoise state after creating second account: {:?}",
        whitenoise
    );

    assert_eq!(whitenoise.accounts.len(), 2);
    assert_eq!(whitenoise.active_account, Some(created_account_2.pubkey));
    tracing::info!("Second account created and set as active");

    tracing::info!("Logging out second account...");
    whitenoise.logout(&created_account_2).await?;
    tracing::debug!(
        "Whitenoise state after logging out second account: {:?}",
        whitenoise
    );

    assert_eq!(whitenoise.accounts.len(), 1);
    assert!(whitenoise.accounts.contains_key(&created_account.pubkey));
    assert!(!whitenoise.accounts.contains_key(&created_account_2.pubkey));
    assert_eq!(whitenoise.active_account, Some(created_account.pubkey));
    tracing::info!("Second account logged out and first account set as active");

    tracing::info!("=== Testing login with known account that has published events ===");

    // Generate a known keypair
    let known_keys = Keys::generate();
    let known_pubkey = known_keys.public_key();
    tracing::info!(
        "Generated known keypair with pubkey: {}",
        known_pubkey.to_hex()
    );

    // Create a direct nostr-sdk client to publish events before login
    tracing::info!("Creating direct nostr client to publish events...");
    let test_client = Client::default();

    // Add development relays (same as used by whitenoise in development)
    let dev_relays = vec!["ws://localhost:8080", "ws://localhost:7777"];

    for relay in &dev_relays {
        test_client.add_relay(*relay).await.unwrap();
    }

    // Connect to relays
    tracing::info!("Connecting to relays...");
    test_client.connect().await;

    // Set the signer for our known keys
    test_client.set_signer(known_keys.clone()).await;

    // Wait a moment for connections to establish
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // Create and publish a metadata event
    let metadata = Metadata {
        name: Some("Test User".to_string()),
        display_name: Some("Test User".to_string()),
        about: Some("Test account for integration testing".to_string()),
        picture: Some("https://example.com/avatar.jpg".to_string()),
        ..Default::default()
    };

    let metadata_event = EventBuilder::metadata(&metadata);

    tracing::info!("Publishing metadata event...");
    let metadata_result = test_client.send_event_builder(metadata_event).await;
    tracing::debug!("Published metadata event: {:?}", metadata_result);

    // Create and publish relay list events
    let relay_urls: Vec<String> = dev_relays.iter().map(|s| s.to_string()).collect();

    // Publish nostr relay list (NIP-65)
    let nostr_relay_tags: Vec<Tag> = relay_urls
        .iter()
        .map(|url| Tag::custom(TagKind::Relay, [url.clone()]))
        .collect();
    let nostr_relay_event = EventBuilder::new(Kind::RelayList, "").tags(nostr_relay_tags);

    tracing::info!("Publishing nostr relay list...");
    let nostr_relay_result = test_client.send_event_builder(nostr_relay_event).await;
    tracing::debug!("Published nostr relay list: {:?}", nostr_relay_result);

    // Publish inbox relay list (NIP-17)
    let inbox_relay_tags: Vec<Tag> = relay_urls
        .iter()
        .map(|url| Tag::custom(TagKind::Relay, [url.clone()]))
        .collect();
    let inbox_relay_event = EventBuilder::new(Kind::InboxRelays, "").tags(inbox_relay_tags);

    tracing::info!("Publishing inbox relay list...");
    let inbox_relay_result = test_client.send_event_builder(inbox_relay_event).await;
    tracing::debug!("Published inbox relay list: {:?}", inbox_relay_result);

    // Publish key package relay list (NIP-104)
    let key_package_relay_tags: Vec<Tag> = relay_urls
        .iter()
        .map(|url| Tag::custom(TagKind::Relay, [url.clone()]))
        .collect();
    let key_package_relay_event =
        EventBuilder::new(Kind::MlsKeyPackageRelays, "").tags(key_package_relay_tags);

    tracing::info!("Publishing key package relay list...");
    let key_package_relay_result = test_client
        .send_event_builder(key_package_relay_event)
        .await;
    tracing::debug!(
        "Published key package relay list: {:?}",
        key_package_relay_result
    );

    // Clean up the test client
    tracing::info!("Disconnecting test client...");
    test_client.disconnect().await;

    // Now login with the known keys and verify that the background fetch retrieves the published events
    tracing::info!("Logging in with known keys to test background fetch...");
    let private_key_hex = known_keys.secret_key().to_secret_hex();
    let restored_account = whitenoise.login(private_key_hex).await?;

    tracing::debug!("Logged in account: {:?}", restored_account);
    tracing::debug!("Whitenoise state after login: {:?}", whitenoise);

    // Verify the account was added and set as active
    assert_eq!(whitenoise.accounts.len(), 2);
    assert_eq!(whitenoise.active_account, Some(restored_account.pubkey));
    assert_eq!(restored_account.pubkey, known_pubkey);
    tracing::info!("Account was added and set as active");

    // Wait a moment for background fetch to complete
    tracing::info!("Pausing for background fetch to complete...");
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Re-query the onboarding state to check if background fetch updated the cached data
    tracing::info!("Re-querying onboarding state after background fetch...");
    let updated_onboarding_state = whitenoise
        .load_onboarding_state(restored_account.pubkey)
        .await?;
    tracing::debug!(
        "Updated onboarding state after background fetch: {:?}",
        updated_onboarding_state
    );
    assert!(updated_onboarding_state.inbox_relays);
    assert!(updated_onboarding_state.key_package_relays);
    assert!(!updated_onboarding_state.key_package_published);
    tracing::info!("OnboardingState verified after background fetch");

    // Load the metadata for the restored account to verify it was fetched via background fetch
    tracing::info!("Loading metadata for restored account to test background fetch...");
    let loaded_metadata = whitenoise.load_metadata(restored_account.pubkey).await?;

    if let Some(metadata) = loaded_metadata {
        tracing::debug!("Loaded metadata: {:?}", metadata);
        tracing::info!("Metadata was correctly fetched via background fetch");

        // Verify the metadata matches what we published
        assert_eq!(
            metadata.name,
            Some("Test User".to_string()),
            "Metadata name should match what we published"
        );
        assert_eq!(
            metadata.display_name,
            Some("Test User".to_string()),
            "Metadata display_name should match what we published"
        );
        assert_eq!(
            metadata.about,
            Some("Test account for integration testing".to_string()),
            "Metadata about should match what we published"
        );
        assert_eq!(
            metadata.picture,
            Some("https://example.com/avatar.jpg".to_string()),
            "Metadata picture should match what we published"
        );
        tracing::info!("All metadata fields match the published values");
    } else {
        tracing::error!("Metadata was not found - background fetch may have failed");
    }

    // TODO: Test relay list loading
    // TODO: Test nsec export

    Ok(())
}
