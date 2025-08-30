use crate::integration_tests::core::*;
use crate::WhitenoiseError;
use async_trait::async_trait;
use nostr_sdk::prelude::*;

/// Test case for publishing metadata updates via external client
pub struct PublishMetadataUpdateTestCase {
    account_name: String,
    updated_metadata: Metadata,
}

impl PublishMetadataUpdateTestCase {
    pub fn new(account_name: &str, updated_metadata: Metadata) -> Self {
        Self {
            account_name: account_name.to_string(),
            updated_metadata,
        }
    }
}

#[async_trait]
impl TestCase for PublishMetadataUpdateTestCase {
    async fn run(&self, context: &mut ScenarioContext) -> Result<(), WhitenoiseError> {
        tracing::info!(
            "Publishing metadata update via external client for account: {}",
            self.account_name
        );

        // Get account and export its keys
        let account = context.get_account(&self.account_name)?;
        let nsec = context.whitenoise.export_account_nsec(account).await?;
        let keys = Keys::parse(&nsec)?;

        // Convert dev_relays from &str to RelayUrl
        let dev_relay_urls: Vec<RelayUrl> = context
            .dev_relays
            .iter()
            .map(|url| RelayUrl::parse(url).unwrap())
            .collect();

        // Create external client
        let test_client = create_test_client(&context.dev_relays, keys.clone()).await?;
        let relay_urls: Vec<String> = dev_relay_urls.iter().map(|url| url.to_string()).collect();
        publish_relay_lists(&test_client, relay_urls).await?;

        // Wait an additional second to ensure external event has newer timestamp than initial account metadata
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        // Publish metadata update
        test_client
            .send_event_builder(EventBuilder::metadata(&self.updated_metadata))
            .await
            .unwrap();

        tracing::info!("✓ Metadata update published via external client");

        // Disconnect client
        test_client.disconnect().await;

        // Give events time to deliver and process
        tokio::time::sleep(tokio::time::Duration::from_millis(600)).await;

        // Verify metadata was updated via event processor
        let account = context.get_account(&self.account_name)?;
        let updated_metadata = account.metadata(context.whitenoise).await?;

        assert_eq!(
            updated_metadata.name, self.updated_metadata.name,
            "Subscription-driven metadata update did not apply"
        );

        tracing::info!("✓ Subscription-driven metadata update verified");
        Ok(())
    }
}
