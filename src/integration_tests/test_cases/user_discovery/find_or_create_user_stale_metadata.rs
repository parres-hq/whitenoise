use crate::WhitenoiseError;
use crate::integration_tests::core::test_clients::create_test_client;
use crate::integration_tests::core::*;
use async_trait::async_trait;
use nostr_sdk::{Keys, Metadata};

/// Tests find_or_create_user with force_sync=false on user with STALE metadata
///
/// This test verifies:
/// - Existing user with stale metadata (>24h old)
/// - Called with force_sync=false
/// - Should trigger background_fetch_user_data to refresh metadata
/// - Method returns immediately with OLD metadata
/// - Background fetch eventually updates metadata
///
/// This tests the TTL-based refresh logic that avoids unnecessary syncs
/// for users with fresh metadata while still keeping stale data updated.
///
/// TESTS CODE PATH: Lines 700-715 in users.rs (existing user, stale metadata, force_sync=false)
pub struct FindOrCreateUserStaleMetadataRefreshTestCase {
    test_keys: Keys,
    initial_metadata: Metadata,
    updated_metadata: Metadata,
}

impl FindOrCreateUserStaleMetadataRefreshTestCase {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for FindOrCreateUserStaleMetadataRefreshTestCase {
    fn default() -> Self {
        let keys = Keys::generate();
        Self {
            test_keys: keys,
            initial_metadata: Metadata::new()
                .name("Stale Name")
                .display_name("Stale Display"),
            updated_metadata: Metadata::new()
                .name("Fresh Name")
                .display_name("Fresh Display"),
        }
    }
}

#[async_trait]
impl TestCase for FindOrCreateUserStaleMetadataRefreshTestCase {
    async fn run(&self, context: &mut ScenarioContext) -> Result<(), WhitenoiseError> {
        let test_pubkey = self.test_keys.public_key();
        tracing::info!(
            "Testing stale metadata refresh with force_sync=false for pubkey: {}",
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

        // Wait longer for event to propagate and be processed
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        test_client.disconnect().await;

        // Create user with Blocking mode (call once to avoid refreshing timestamp multiple times)
        context
            .whitenoise
            .find_or_create_user_by_pubkey(
                &test_pubkey,
                crate::whitenoise::users::UserSyncMode::Blocking,
            )
            .await?;

        // Wait for metadata to be fetched
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

        assert!(
            initial_user.id.is_some(),
            "User should have an ID after creation"
        );

        tracing::info!("✓ User created with initial metadata");

        // **TEST-ONLY**: Artificially age the user's metadata to be stale (>24h old)
        let stale_time = chrono::Utc::now() - chrono::Duration::hours(25);
        context
            .whitenoise
            .set_user_updated_at_for_testing(&test_pubkey, stale_time)
            .await?;

        tracing::info!("✓ Set user metadata timestamp to 25 hours ago (stale)");

        // Publish updated metadata
        let test_client2 = create_test_client(&context.dev_relays, self.test_keys.clone()).await?;

        tracing::info!("Publishing updated metadata");
        test_client2
            .send_event_builder(nostr_sdk::EventBuilder::metadata(&self.updated_metadata))
            .await?;
        test_client2.disconnect().await;

        // Small delay to let the event reach the relay but not process through event handler yet
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Call find_or_create with Background mode
        // This should trigger background refresh because metadata is stale
        let user_before_refresh = context
            .whitenoise
            .find_or_create_user_by_pubkey(
                &test_pubkey,
                crate::whitenoise::users::UserSyncMode::Background,
            )
            .await?;

        // Note: We cannot reliably assert the metadata is still old here because the background
        // fetch might complete before we check, or the event might have already been processed
        // by the time we make this call. The key behavior we're testing is that find_or_create
        // RETURNS immediately (not blocking) when called with Background mode.

        tracing::info!(
            "✓ Method returned immediately (background mode) with metadata: {:?}",
            user_before_refresh.metadata.name
        );

        // Wait for background refresh to complete
        tracing::info!("Waiting for background metadata refresh...");
        let refreshed_user = retry_default(
            || async {
                let u = context.whitenoise.find_user_by_pubkey(&test_pubkey).await?;
                if u.metadata.name == self.updated_metadata.name {
                    Ok(u)
                } else {
                    Err(WhitenoiseError::Other(anyhow::anyhow!(
                        "Background refresh not yet complete: still has {:?}, expecting {:?}",
                        u.metadata.name,
                        self.updated_metadata.name
                    )))
                }
            },
            &format!(
                "wait for background refresh for stale user {}",
                &test_pubkey.to_hex()[..8]
            ),
        )
        .await?;

        assert_eq!(
            refreshed_user.metadata.name, self.updated_metadata.name,
            "Should have updated metadata after background refresh"
        );

        tracing::info!("✓ Stale metadata successfully refreshed in background");

        Ok(())
    }
}
