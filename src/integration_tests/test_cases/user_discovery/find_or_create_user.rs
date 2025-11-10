use crate::WhitenoiseError;
use crate::integration_tests::core::test_clients::{create_test_client, publish_relay_lists};
use crate::integration_tests::core::*;
use async_trait::async_trait;
use nostr_sdk::{Keys, Metadata, RelayUrl};

/// Tests find_or_create_user with force_sync=true (synchronous/blocking mode)
///
/// This test verifies:
/// - User creation when user doesn't exist
/// - Synchronous metadata fetching (blocking until complete)
/// - Synchronous relay list fetching (blocking until complete)
/// - Idempotency (calling twice returns the same user)
///
/// LIMITATION: This test only covers force_sync=true. For force_sync=false
/// (background mode), see FindOrCreateUserBackgroundModeTestCase.
pub struct FindOrCreateUserTestCase {
    test_keys: Keys,
    should_have_metadata: bool,
    should_have_relays: bool,
    test_metadata: Option<Metadata>,
    test_relays: Vec<RelayUrl>,
}

impl FindOrCreateUserTestCase {
    pub fn basic() -> Self {
        let keys = Keys::generate();
        Self {
            test_keys: keys,
            should_have_metadata: false,
            should_have_relays: false,
            test_metadata: None,
            test_relays: vec![],
        }
    }

    pub fn with_metadata(mut self) -> Self {
        let metadata = Metadata::new()
            .name("Test User")
            .display_name("Test Display Name")
            .about("Test about section");

        self.should_have_metadata = true;
        self.test_metadata = Some(metadata);
        self
    }

    pub fn with_relays(mut self) -> Self {
        let test_relays = if cfg!(debug_assertions) {
            vec![
                RelayUrl::parse("ws://localhost:8080").unwrap(),
                RelayUrl::parse("ws://localhost:7777").unwrap(),
            ]
        } else {
            vec![
                RelayUrl::parse("wss://relay.damus.io").unwrap(),
                RelayUrl::parse("wss://relay.primal.net").unwrap(),
                RelayUrl::parse("wss://nos.lol").unwrap(),
            ]
        };

        self.should_have_relays = true;
        self.test_relays = test_relays;
        self
    }

    async fn publish_metadata(&self, context: &ScenarioContext) -> Result<(), WhitenoiseError> {
        let test_client = create_test_client(&context.dev_relays, self.test_keys.clone()).await?;

        if let Some(metadata) = &self.test_metadata {
            tracing::info!("Publishing test metadata for test pubkey");
            test_client
                .send_event_builder(nostr_sdk::EventBuilder::metadata(metadata))
                .await?;
        }

        test_client.disconnect().await;
        Ok(())
    }

    async fn publish_relays_data(&self, context: &ScenarioContext) -> Result<(), WhitenoiseError> {
        let test_client = create_test_client(&context.dev_relays, self.test_keys.clone()).await?;

        tracing::info!("Publishing test relay list for test pubkey");
        let relay_urls: Vec<String> = self.test_relays.iter().map(|url| url.to_string()).collect();
        publish_relay_lists(&test_client, relay_urls).await?;

        test_client.disconnect().await;
        Ok(())
    }
}

#[async_trait]
impl TestCase for FindOrCreateUserTestCase {
    async fn run(&self, context: &mut ScenarioContext) -> Result<(), WhitenoiseError> {
        let test_pubkey = self.test_keys.public_key();
        tracing::info!("Testing find_or_create_user for pubkey: {}", test_pubkey);
        let user_exists = context
            .whitenoise
            .find_user_by_pubkey(&test_pubkey)
            .await
            .is_ok();
        assert!(!user_exists, "User should not exist initially");

        if self.should_have_metadata || self.should_have_relays {
            // Create an account: We need to have at least one account to be able to subscribe to events
            context.whitenoise.create_identity().await?;
        }

        if self.should_have_metadata {
            self.publish_metadata(context).await?;
        }

        if self.should_have_relays {
            self.publish_relays_data(context).await?;
        }

        let mut user = context
            .whitenoise
            .find_or_create_user_by_pubkey(&test_pubkey, true) // force synchronous metadata sync
            .await?;

        assert_eq!(user.pubkey, test_pubkey, "User pubkey should match");
        assert!(user.id.is_some(), "User should have an ID after creation");

        tracing::info!(
            "✓ User created with ID: {} for pubkey: {}",
            user.id.unwrap(),
            test_pubkey
        );

        let found_user = context.whitenoise.find_user_by_pubkey(&test_pubkey).await?;
        assert_eq!(found_user.pubkey, test_pubkey, "Found user should match");
        assert_eq!(found_user.id, user.id, "Found user ID should match");

        tracing::info!("✓ User can be found by pubkey after creation");

        // If we expect metadata, wait until it arrives (background fetch is now asynchronous)
        if self.should_have_metadata {
            tracing::info!("Waiting for background metadata fetch to complete...");
            user = retry_default(
                || async {
                    let updated_user = context.whitenoise.find_user_by_pubkey(&test_pubkey).await?;
                    if updated_user.metadata != nostr_sdk::Metadata::default() {
                        Ok(updated_user)
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
        }

        if self.should_have_metadata {
            if let Some(expected_metadata) = &self.test_metadata {
                assert_eq!(
                    user.metadata.name, expected_metadata.name,
                    "Metadata name should match published data"
                );
                assert_eq!(
                    user.metadata.display_name, expected_metadata.display_name,
                    "Metadata display_name should match published data"
                );
                assert_eq!(
                    user.metadata.about, expected_metadata.about,
                    "Metadata about should match published data"
                );

                tracing::info!(
                    "✓ User metadata matches published data: name={:?}, display_name={:?}",
                    user.metadata.name,
                    user.metadata.display_name
                );
            }
        } else {
            assert!(
                user.metadata.name.is_none() || user.metadata.name == Some(String::new()),
                "User should have empty/no name when no metadata published"
            );
            tracing::info!("✓ User has empty metadata as expected (nothing published)");
        }

        if self.should_have_relays {
            tracing::info!("Waiting for background relay fetch to complete...");

            // Wait for background relay fetching to complete
            let user_relays = retry_default(
                || async {
                    let updated_user = context.whitenoise.find_user_by_pubkey(&test_pubkey).await?;
                    let relays = updated_user
                        .relays_by_type(
                            crate::whitenoise::relays::RelayType::Nip65,
                            context.whitenoise,
                        )
                        .await?;

                    if relays.is_empty() {
                        Err(WhitenoiseError::Other(anyhow::anyhow!(
                            "Background relay fetch not yet complete"
                        )))
                    } else {
                        Ok(relays)
                    }
                },
                &format!(
                    "wait for background relay fetch for user {}",
                    &test_pubkey.to_hex()[..8]
                ),
            )
            .await?;

            let relay_urls: Vec<&RelayUrl> = user_relays.iter().map(|r| &r.url).collect();
            for expected_relay in &self.test_relays {
                assert!(
                    relay_urls.contains(&expected_relay),
                    "User should have relay {} that was published",
                    expected_relay
                );
            }

            tracing::info!(
                "✓ User relay list matches published data: {} relays found",
                user_relays.len()
            );
        } else {
            tracing::info!("✓ No relay publication needed for this test case");
        }

        // Second call with force_sync=false to test idempotency
        // NOTE: Since the user was just created/synced, metadata is fresh (<24h),
        // so this call will return immediately without any sync (lines 716-723 in users.rs)
        // However, we CANNOT verify that sync was skipped - we can only verify the user is returned.
        let user_again = context
            .whitenoise
            .find_or_create_user_by_pubkey(&test_pubkey, false)
            .await?;
        assert_eq!(
            user_again.id, user.id,
            "Should return same user on second call"
        );
        assert_eq!(
            user_again.pubkey, user.pubkey,
            "Should return same user pubkey"
        );

        tracing::info!("✓ find_or_create returns existing user on second call");

        Ok(())
    }
}

/// Tests find_or_create_user with force_sync=false for a NEW user (background mode)
///
/// This test verifies:
/// - User is created immediately in the database
/// - Method returns immediately WITHOUT waiting for metadata/relays
/// - Metadata is empty immediately after the call
/// - Background fetch eventually completes and populates metadata
///
/// This is the KEY test that shows the difference between force_sync=true and false:
/// - force_sync=true: blocks until metadata is fetched
/// - force_sync=false: returns immediately, fetches in background
///
/// TESTS CODE PATH: Lines 691-699 in users.rs (created=true, force_sync=false)
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
            "Testing find_or_create_user with force_sync=false for pubkey: {}",
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

        // Call with force_sync=false (background mode)
        let user = context
            .whitenoise
            .find_or_create_user_by_pubkey(&test_pubkey, false)
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
        test_client.disconnect().await;

        // Create user with force_sync to get initial metadata
        let initial_user = context
            .whitenoise
            .find_or_create_user_by_pubkey(&test_pubkey, true)
            .await?;

        assert_eq!(
            initial_user.metadata.name, self.initial_metadata.name,
            "Should have initial metadata"
        );

        tracing::info!("✓ User created with initial metadata");

        // Small delay to ensure the first metadata event is fully processed
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Publish updated metadata
        let test_client2 = create_test_client(&context.dev_relays, self.test_keys.clone()).await?;

        tracing::info!("Publishing updated metadata");
        test_client2
            .send_event_builder(nostr_sdk::EventBuilder::metadata(&self.updated_metadata))
            .await?;

        // Wait for the event to be published and propagate
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        test_client2.disconnect().await;

        // Call find_or_create with force_sync=true on existing user
        // This should force a sync even though metadata was just fetched
        // NOTE: We use retry here because the relay might take a moment to make the event available
        let updated_user = retry_default(
            || async {
                let user = context
                    .whitenoise
                    .find_or_create_user_by_pubkey(&test_pubkey, true)
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
                "wait for force sync to fetch updated metadata for user {}",
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

        // Wait for event to propagate to relay
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        test_client.disconnect().await;

        // Create user with force_sync to get initial metadata
        let initial_user = context
            .whitenoise
            .find_or_create_user_by_pubkey(&test_pubkey, true)
            .await?;

        // Use detailed error if metadata wasn't fetched
        if initial_user.metadata.name != self.initial_metadata.name {
            return Err(WhitenoiseError::Other(anyhow::anyhow!(
                "Failed to fetch initial metadata. Got {:?}, expected {:?}. \
                This might be a timing issue with relay propagation or the event was already processed.",
                initial_user.metadata.name,
                self.initial_metadata.name
            )));
        }

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

        // Call find_or_create with force_sync=false
        // This should trigger background refresh because metadata is stale
        let user_before_refresh = context
            .whitenoise
            .find_or_create_user_by_pubkey(&test_pubkey, false)
            .await?;

        // Should return immediately with OLD metadata (background fetch hasn't completed yet)
        assert_eq!(
            user_before_refresh.metadata.name, self.initial_metadata.name,
            "Should still have old metadata immediately after call (background mode)"
        );

        tracing::info!("✓ Method returned immediately with stale metadata");

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

// ============================================================================
// DOCUMENTATION: Remaining Test Limitation
// ============================================================================
//
// There is one scenario that still CANNOT be fully tested:
//
// FRESH METADATA SKIP (lines 716-723 in users.rs)
//    Scenario: existing user + fresh metadata (<24h) + force_sync=false
//    Expected: Should skip sync entirely (neither blocking nor background)
//
//    Why we can't fully test:
//    - Cannot verify sync was NOT called (no mocking or observable metrics)
//    - FindOrCreateUserTestCase line 242 tests this scenario but can only
//      verify the user is returned, not that sync was definitively skipped
//    - No way to distinguish "skipped" from "attempted and failed silently"
//
//    What IS tested:
//    - TTL calculation logic in test_needs_metadata_refresh_fresh_metadata
//    - User is returned correctly (but not the skip behavior itself)
//
// To properly test this scenario would require:
// - Spy/mock objects to intercept background_fetch_user_data calls
// - Metrics/observability to track sync attempts
// - Or observable side effects that differ between "skipped" and "attempted"
//
// ============================================================================
