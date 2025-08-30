use crate::integration_tests::core::*;
use crate::{RelayType, WhitenoiseError};
use async_trait::async_trait;
use nostr_sdk::prelude::*;

/// Test case for publishing relay list updates via external client
pub struct PublishRelayListUpdateTestCase {
    account_name: String,
    new_relay_url: String,
}

impl PublishRelayListUpdateTestCase {
    pub fn new(account_name: &str, new_relay_url: String) -> Self {
        Self {
            account_name: account_name.to_string(),
            new_relay_url,
        }
    }
}

#[async_trait]
impl TestCase for PublishRelayListUpdateTestCase {
    async fn run(&self, context: &mut ScenarioContext) -> Result<(), WhitenoiseError> {
        tracing::info!(
            "Publishing relay list update via external client for account: {}",
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

        // Publish relay list update (NIP-65)
        let nip65_update_tags = vec![Tag::custom(
            TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::R)),
            [self.new_relay_url.clone()],
        )];
        test_client
            .send_event_builder(EventBuilder::new(Kind::RelayList, "").tags(nip65_update_tags))
            .await
            .unwrap();

        tracing::info!("✓ Relay list update published via external client");

        // Disconnect client
        test_client.disconnect().await;

        // Give subscriptions time to deliver and process relay list updates
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

        // Verify relay list was updated via event processor
        let account = context.get_account(&self.account_name)?;
        let user = context
            .whitenoise
            .find_user_by_pubkey(&account.pubkey)
            .await?;

        let nip65_relays = user
            .relays_by_type(RelayType::Nip65, context.whitenoise)
            .await?;

        let expected_relay = RelayUrl::parse(&self.new_relay_url).unwrap();
        let has_new_relay = nip65_relays.iter().any(|r| r.url == expected_relay);

        assert!(
            has_new_relay,
            "NIP-65 relays should include subscription-updated relay: {}",
            self.new_relay_url
        );

        tracing::info!("✓ Subscription-driven relay list update verified");
        Ok(())
    }
}
