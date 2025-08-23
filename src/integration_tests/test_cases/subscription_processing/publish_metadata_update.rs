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
        let test_client = Client::default();
        for relay in &dev_relay_urls {
            test_client.add_relay(relay.clone()).await.unwrap();
        }
        test_client.connect().await;
        test_client.set_signer(keys).await;

        // Wait for client to connect
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        // Publish metadata update
        test_client
            .send_event_builder(EventBuilder::metadata(&self.updated_metadata))
            .await
            .unwrap();

        tracing::info!("✓ Metadata update published via external client");

        // Disconnect client
        test_client.disconnect().await;

        // Give events time to deliver and process
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Verify metadata was updated via event processor
        let account = context.get_account(&self.account_name)?;
        let updated_metadata = account.metadata(context.whitenoise).await?;

        let expected_name = self.updated_metadata.name.clone().unwrap_or_default();
        assert_eq!(
            updated_metadata.name,
            Some(expected_name.clone()),
            "Subscription-driven metadata update did not apply"
        );

        tracing::info!("✓ Subscription-driven metadata update verified");
        Ok(())
    }
}
