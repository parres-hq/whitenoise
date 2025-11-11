use crate::WhitenoiseError;
use crate::integration_tests::core::test_clients::{create_test_client, publish_relay_lists};
use crate::integration_tests::core::*;
use async_trait::async_trait;
use nostr_sdk::{Keys, Metadata, RelayUrl};

/// Tests find_or_create_user with Background mode for a NEW user
///
/// This test verifies:
/// - User is created immediately in the database
/// - Method returns immediately WITHOUT waiting for metadata/relays
/// - Metadata is empty immediately after the call
/// - Background fetch eventually completes and populates metadata
///
/// This is the KEY test that shows the difference between Blocking and Background modes:
/// - Blocking: blocks until metadata is fetched
/// - Background: returns immediately, fetches in background
///
/// TESTS CODE PATH: Lines 691-699 in users.rs (created=true, Background mode)
pub struct FindOrCreateUserBackgroundModeTestCase {
    test_keys: Keys,
    test_metadata: Metadata,
    test_relays: Vec<RelayUrl>,
}

impl FindOrCreateUserBackgroundModeTestCase {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for FindOrCreateUserBackgroundModeTestCase {
    fn default() -> Self {
        let keys = Keys::generate();
        let metadata = Metadata::new()
            .name("Background User")
            .display_name("Background Display")
            .about("Testing background mode");

        let test_relays = if cfg!(debug_assertions) {
            vec![
                RelayUrl::parse("ws://localhost:8080").unwrap(),
                RelayUrl::parse("ws://localhost:7777").unwrap(),
            ]
        } else {
            vec![
                RelayUrl::parse("wss://relay.damus.io").unwrap(),
                RelayUrl::parse("wss://relay.primal.net").unwrap(),
            ]
        };

        Self {
            test_keys: keys,
            test_metadata: metadata,
            test_relays,
        }
    }
}

#[async_trait]
impl TestCase for FindOrCreateUserBackgroundModeTestCase {
    async fn run(&self, context: &mut ScenarioContext) -> Result<(), WhitenoiseError> {
        let test_pubkey = self.test_keys.public_key();
        tracing::info!(
            "Testing find_or_create_user with UserSyncMode::Background for pubkey: {}",
            test_pubkey
        );

        // Create an identity so we can subscribe to events
        context.whitenoise.create_identity().await?;

        // Publish test data
        let test_client = create_test_client(&context.dev_relays, self.test_keys.clone()).await?;

        tracing::info!("Publishing test metadata and relays for test pubkey");
        test_client
            .send_event_builder(nostr_sdk::EventBuilder::metadata(&self.test_metadata))
            .await?;

        let relay_urls: Vec<String> = self.test_relays.iter().map(|url| url.to_string()).collect();
        publish_relay_lists(&test_client, relay_urls).await?;
        test_client.disconnect().await;

        // Call with Background mode
        let user = context
            .whitenoise
            .find_or_create_user_by_pubkey(
                &test_pubkey,
                crate::whitenoise::users::UserSyncMode::Background,
            )
            .await?;

        assert_eq!(user.pubkey, test_pubkey, "User pubkey should match");
        assert!(user.id.is_some(), "User should have an ID after creation");

        tracing::info!(
            "✓ User created with background sync: ID {} for pubkey: {}",
            user.id.unwrap(),
            test_pubkey
        );

        // The user should be created immediately, but metadata should be empty initially
        assert_eq!(
            user.metadata,
            nostr_sdk::Metadata::default(),
            "Metadata should be empty immediately after background mode call"
        );

        tracing::info!("✓ Background mode returns immediately without metadata");

        // Wait for background fetch to complete
        tracing::info!("Waiting for background metadata fetch to complete...");
        let updated_user = retry_default(
            || async {
                let u = context.whitenoise.find_user_by_pubkey(&test_pubkey).await?;
                if u.metadata != nostr_sdk::Metadata::default() {
                    Ok(u)
                } else {
                    Err(WhitenoiseError::Other(anyhow::anyhow!(
                        "Background metadata fetch not yet complete"
                    )))
                }
            },
            &format!(
                "wait for background metadata fetch for user {}",
                &test_pubkey.to_hex()[..8]
            ),
        )
        .await?;

        assert_eq!(
            updated_user.metadata.name, self.test_metadata.name,
            "Metadata name should match after background fetch"
        );

        tracing::info!("✓ Background fetch completed successfully");

        Ok(())
    }
}
