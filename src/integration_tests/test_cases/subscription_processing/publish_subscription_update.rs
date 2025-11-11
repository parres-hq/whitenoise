use crate::integration_tests::core::*;
use crate::{RelayType, WhitenoiseError};
use async_trait::async_trait;
use nostr_sdk::prelude::*;
use std::collections::HashSet;

/// Test case for subscription-driven updates using builder pattern
pub struct PublishSubscriptionUpdateTestCase {
    keys: Keys,
    account_name: Option<String>,
    metadata: Option<Metadata>,
    new_relay_url: Option<String>,
    follow_pubkeys: Option<Vec<PublicKey>>,
}

impl PublishSubscriptionUpdateTestCase {
    /// Create test case for account-based updates
    pub fn for_account(account_name: &str) -> Self {
        Self {
            keys: Keys::generate(), // Placeholder - will be replaced with account keys
            account_name: Some(account_name.to_string()),
            metadata: None,
            new_relay_url: None,
            follow_pubkeys: None,
        }
    }

    /// Create test case for external user updates
    pub fn for_external_user(keys: Keys) -> Self {
        Self {
            keys,
            account_name: None,
            metadata: None,
            new_relay_url: None,
            follow_pubkeys: None,
        }
    }

    /// Add metadata update to the test
    pub fn with_metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Add relay list update to the test
    pub fn with_relay_update(mut self, new_relay_url: String) -> Self {
        self.new_relay_url = Some(new_relay_url);
        self
    }

    /// Add follow list update to the test (account-based only)
    pub fn with_follow_list(mut self, follows: Vec<PublicKey>) -> Self {
        self.follow_pubkeys = Some(follows);
        self
    }

    /// Setup test client with relay lists published
    async fn setup_test_client(
        context: &ScenarioContext,
        keys: &Keys,
    ) -> Result<Client, WhitenoiseError> {
        let test_client = create_test_client(&context.dev_relays, keys.clone()).await?;
        let relay_urls: Vec<String> = context
            .dev_relays
            .iter()
            .map(|url| url.to_string())
            .collect();
        publish_relay_lists(&test_client, relay_urls).await?;
        tokio::time::sleep(tokio::time::Duration::from_millis(600)).await;
        Ok(test_client)
    }

    /// Get the appropriate keys (account keys or external keys)
    async fn get_keys(&self, context: &ScenarioContext) -> Result<Keys, WhitenoiseError> {
        if let Some(account_name) = &self.account_name {
            let account = context.get_account(account_name)?;
            let nsec = context.whitenoise.export_account_nsec(account).await?;
            Ok(Keys::parse(&nsec)?)
        } else {
            Ok(self.keys.clone())
        }
    }

    /// Ensure external user exists before testing
    async fn ensure_external_user_exists(
        &self,
        context: &mut ScenarioContext,
        keys: &Keys,
    ) -> Result<(), WhitenoiseError> {
        if self.account_name.is_none() {
            let pubkey = keys.public_key();
            let initial_user = context
                .whitenoise
                .find_or_create_user_by_pubkey(
                    &pubkey,
                    crate::whitenoise::users::UserSyncMode::Background,
                )
                .await?;

            // Verify initial state for metadata tests
            if self.metadata.is_some() {
                assert!(
                    initial_user.metadata.name.is_none()
                        || initial_user.metadata.name == Some(String::new()),
                    "Initial external user should have no name metadata"
                );
            }
        }
        Ok(())
    }

    /// Publish metadata update
    async fn publish_metadata(
        &self,
        test_client: &Client,
        metadata: &Metadata,
    ) -> Result<(), WhitenoiseError> {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        test_client
            .send_event_builder(EventBuilder::metadata(metadata))
            .await
            .unwrap();

        tracing::info!("✓ Metadata update published via external client");
        Ok(())
    }

    /// Publish relay list update
    async fn publish_relay_list(
        &self,
        test_client: &Client,
        context: &ScenarioContext,
        new_relay_url: &str,
    ) -> Result<(), WhitenoiseError> {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        // Create complete relay list (dev relays + new relay)
        let mut relay_urls: Vec<String> = context
            .dev_relays
            .iter()
            .map(|url| url.to_string())
            .collect();
        relay_urls.push(new_relay_url.to_string());

        let nip65_tags: Vec<Tag> = relay_urls
            .iter()
            .map(|relay_url| {
                Tag::custom(
                    TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::R)),
                    [relay_url],
                )
            })
            .collect();

        test_client
            .send_event_builder(EventBuilder::new(Kind::RelayList, "").tags(nip65_tags))
            .await
            .unwrap();

        tracing::info!("✓ Relay list update published via external client");
        Ok(())
    }

    /// Publish follow list update (account-based)
    async fn publish_follow_list_update(
        &self,
        test_client: &Client,
        follows: &[PublicKey],
    ) -> Result<(), WhitenoiseError> {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        publish_follow_list(test_client, follows).await?;
        tracing::info!("✓ Follow list published via test client");
        Ok(())
    }

    /// Verify metadata update
    async fn verify_metadata(
        &self,
        context: &mut ScenarioContext,
        expected_metadata: &Metadata,
        keys: &Keys,
    ) -> Result<(), WhitenoiseError> {
        if let Some(account_name) = &self.account_name {
            // Account-based verification
            let account = context.get_account(account_name)?;
            let updated_metadata = account.metadata(context.whitenoise).await?;

            assert_eq!(
                updated_metadata.name, expected_metadata.name,
                "Account subscription-driven metadata update did not apply"
            );
        } else {
            // External user verification
            let pubkey = keys.public_key();
            let updated_user = context.whitenoise.find_user_by_pubkey(&pubkey).await?;

            assert_eq!(
                updated_user.metadata.name, expected_metadata.name,
                "External user subscription-driven metadata update did not apply"
            );

            if let Some(expected_about) = &expected_metadata.about {
                assert_eq!(
                    updated_user.metadata.about.as_ref(),
                    Some(expected_about),
                    "External user subscription-driven about field did not apply"
                );
            }

            if let Some(expected_display_name) = &expected_metadata.display_name {
                assert_eq!(
                    updated_user.metadata.display_name.as_ref(),
                    Some(expected_display_name),
                    "External user subscription-driven display_name field did not apply"
                );
            }
        }

        tracing::info!("✓ Subscription-driven metadata update verified");
        Ok(())
    }

    /// Verify relay list update
    async fn verify_relay_update(
        &self,
        context: &mut ScenarioContext,
        expected_relay_url: &str,
        keys: &Keys,
    ) -> Result<(), WhitenoiseError> {
        let user = if let Some(account_name) = &self.account_name {
            let account = context.get_account(account_name)?;
            context
                .whitenoise
                .find_user_by_pubkey(&account.pubkey)
                .await?
        } else {
            let pubkey = keys.public_key();
            context.whitenoise.find_user_by_pubkey(&pubkey).await?
        };

        let nip65_relays = user
            .relays_by_type(RelayType::Nip65, context.whitenoise)
            .await?;
        let expected_relay = RelayUrl::parse(expected_relay_url).unwrap();
        let has_new_relay = nip65_relays.iter().any(|r| r.url == expected_relay);

        let user_type = if self.account_name.is_some() {
            "Account"
        } else {
            "External user"
        };

        assert!(
            has_new_relay,
            "{} NIP-65 relays should include subscription-updated relay: {}, got: {:?}",
            user_type, expected_relay_url, nip65_relays
        );

        tracing::info!(
            "✓ {} subscription-driven relay list update verified",
            user_type
        );
        Ok(())
    }

    /// Verify follow list processed into follows (account-based)
    async fn verify_follow_list(
        &self,
        context: &mut ScenarioContext,
        expected_follows: &[PublicKey],
    ) -> Result<(), WhitenoiseError> {
        let Some(account_name) = &self.account_name else {
            return Err(WhitenoiseError::InvalidInput(
                "Contact list verification only supported for accounts".to_string(),
            ));
        };

        let account = context.get_account(account_name)?;
        let follows = context.whitenoise.follows(account).await?;

        let expected: HashSet<PublicKey> = expected_follows.iter().copied().collect();
        let actual: HashSet<PublicKey> = follows.iter().map(|u| u.pubkey).collect();

        assert!(
            actual == expected,
            "Account follows do not match expected follows. Missing: {:?}, Extra: {:?}",
            expected.difference(&actual).collect::<Vec<_>>(),
            actual.difference(&expected).collect::<Vec<_>>()
        );

        tracing::info!("✓ Account follow list exactly matches expected set");
        Ok(())
    }
}

#[async_trait]
impl TestCase for PublishSubscriptionUpdateTestCase {
    async fn run(&self, context: &mut ScenarioContext) -> Result<(), WhitenoiseError> {
        let has_metadata = self.metadata.is_some();
        let has_relay = self.new_relay_url.is_some();
        let has_follows = self.follow_pubkeys.is_some();

        let mut updates_parts = Vec::new();
        if has_metadata {
            updates_parts.push("metadata");
        }
        if has_relay {
            updates_parts.push("relay list");
        }
        if has_follows {
            updates_parts.push("follows");
        }

        if updates_parts.is_empty() {
            return Err(WhitenoiseError::InvalidInput(
                "No updates specified".to_string(),
            ));
        }

        let updates = updates_parts.join(", ");

        // Get appropriate keys first
        let test_keys = self.get_keys(context).await?;

        let user_desc = if let Some(account_name) = &self.account_name {
            format!("account: {}", account_name)
        } else {
            format!("external user: {}", test_keys.public_key())
        };

        tracing::info!(
            "Testing subscription-driven {} updates for {}",
            updates,
            user_desc
        );

        // Ensure external user exists (no-op for accounts)
        self.ensure_external_user_exists(context, &test_keys)
            .await?;

        // Setup test client
        let test_client = Self::setup_test_client(context, &test_keys).await?;

        // Publish updates
        if let Some(metadata) = &self.metadata {
            self.publish_metadata(&test_client, metadata).await?;
        }

        if let Some(relay_url) = &self.new_relay_url {
            self.publish_relay_list(&test_client, context, relay_url)
                .await?;
        }

        if let Some(follows) = &self.follow_pubkeys {
            self.publish_follow_list_update(&test_client, follows)
                .await?;
        }

        // Wait for processing and disconnect
        tokio::time::sleep(tokio::time::Duration::from_millis(600)).await;
        test_client.disconnect().await;

        // Verify updates
        if let Some(metadata) = &self.metadata {
            self.verify_metadata(context, metadata, &test_keys).await?;
        }

        if let Some(relay_url) = &self.new_relay_url {
            self.verify_relay_update(context, relay_url, &test_keys)
                .await?;
        }

        if let Some(follows) = &self.follow_pubkeys {
            self.verify_follow_list(context, follows).await?;
        }

        Ok(())
    }
}
