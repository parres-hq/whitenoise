use crate::WhitenoiseError;
use crate::integration_tests::core::test_clients::create_test_client;
use crate::integration_tests::core::*;
use async_trait::async_trait;
use nostr_sdk::{Keys, Metadata};

/// Tests find_or_create_user with force_sync=true on an EXISTING user
///
/// This test verifies:
/// - force_sync=true ALWAYS syncs metadata, even if just fetched
/// - Ignores the TTL logic (24h freshness check)
/// - Can fetch updated metadata that was published after user creation
///
/// This demonstrates that force_sync=true overrides the TTL optimization
/// and is useful when you know metadata has changed and want it immediately.
///
/// TESTS CODE PATH: Lines 651-688 in users.rs (force_sync=true, existing user)
pub struct FindOrCreateUserForceSyncOnExistingTestCase {
    test_keys: Keys,
    initial_metadata: Metadata,
    updated_metadata: Metadata,
}

impl FindOrCreateUserForceSyncOnExistingTestCase {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for FindOrCreateUserForceSyncOnExistingTestCase {
    fn default() -> Self {
        let keys = Keys::generate();
        Self {
            test_keys: keys,
            initial_metadata: Metadata::new()
                .name("Initial Name")
                .display_name("Initial Display"),
            updated_metadata: Metadata::new()
                .name("Updated Name")
                .display_name("Updated Display"),
        }
    }
}

#[async_trait]
impl TestCase for FindOrCreateUserForceSyncOnExistingTestCase {
    async fn run(&self, context: &mut ScenarioContext) -> Result<(), WhitenoiseError> {
        let test_pubkey = self.test_keys.public_key();
        tracing::info!(
            "Testing force_sync=true on existing user with fresh metadata for pubkey: {}",
            test_pubkey
        );

        // Create identity so we can subscribe
        context.whitenoise.create_identity().await?;

        // Publish initial metadata
        let test_client = create_test_client(&context.dev_relays, self.test_keys.clone()).await?;

        tracing::info!("Publishing initial metadata");
        test_client
            .send_event_builder(nostr_sdk::EventBuilder::metadata(&self.initial_metadata))
            .await?;

        // Wait for event to propagate to relay
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        test_client.disconnect().await;

        // Create user with Blocking mode (call once to avoid refreshing timestamp)
        context
            .whitenoise
            .find_or_create_user_by_pubkey(
                &test_pubkey,
                crate::whitenoise::users::UserSyncMode::Blocking,
            )
            .await?;

        // Wait for metadata to be fetched and processed
        let initial_user = retry_default(
            || async {
                let user = context.whitenoise.find_user_by_pubkey(&test_pubkey).await?;
                if user.metadata.name == self.initial_metadata.name {
                    Ok(user)
                } else {
                    Err(WhitenoiseError::Other(anyhow::anyhow!(
                        "Initial metadata not yet fetched: got {:?}, expecting {:?}",
                        user.metadata.name,
                        self.initial_metadata.name
                    )))
                }
            },
            &format!(
                "wait for initial metadata fetch for user {}",
                &test_pubkey.to_hex()[..8]
            ),
        )
        .await?;

        tracing::info!("✓ User created with initial metadata");

        // Small delay to ensure the first metadata event is fully processed
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        // Publish updated metadata
        let test_client2 = create_test_client(&context.dev_relays, self.test_keys.clone()).await?;

        tracing::info!("Publishing updated metadata");
        test_client2
            .send_event_builder(nostr_sdk::EventBuilder::metadata(&self.updated_metadata))
            .await?;

        // Wait for the event to be published and propagate to relay
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        test_client2.disconnect().await;

        // Call find_or_create with Blocking mode on existing user
        // This should force a sync even though metadata was just fetched
        // NOTE: We use retry here because the relay might take a moment to make the event available
        let updated_user = retry_default(
            || async {
                let user = context
                    .whitenoise
                    .find_or_create_user_by_pubkey(
                        &test_pubkey,
                        crate::whitenoise::users::UserSyncMode::Blocking,
                    )
                    .await?;

                if user.metadata.name == self.updated_metadata.name {
                    Ok(user)
                } else {
                    Err(WhitenoiseError::Other(anyhow::anyhow!(
                        "Updated metadata not yet fetched: still has {:?}, expecting {:?}",
                        user.metadata.name,
                        self.updated_metadata.name
                    )))
                }
            },
            &format!(
                "wait for blocking sync to fetch updated metadata for user {}",
                &test_pubkey.to_hex()[..8]
            ),
        )
        .await?;

        assert_eq!(
            updated_user.id, initial_user.id,
            "Should return same user ID"
        );
        assert_eq!(
            updated_user.metadata.name, self.updated_metadata.name,
            "Should have updated metadata after force sync"
        );

        tracing::info!("✓ Force sync on existing user successfully fetched updated metadata");

        Ok(())
    }
}
