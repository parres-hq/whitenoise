use crate::integration_tests::core::*;
use crate::WhitenoiseError;
use async_trait::async_trait;
use nostr_sdk::prelude::*;

#[derive(Default)]
pub struct LoginWithKnownKeysTestCase;

#[async_trait]
impl TestCase for LoginWithKnownKeysTestCase {
    async fn run(&self, context: &mut ScenarioContext) -> Result<(), WhitenoiseError> {
        tracing::info!("Testing logging in with known keys: known_account");

        let known_keys = Keys::generate();
        let known_pubkey = known_keys.public_key();

        // Publish test events first
        let test_client = create_test_client(&context.dev_relays, known_keys.clone()).await?;
        publish_test_metadata(&test_client, "known_user", "A user with known keys").await?;

        let relay_urls: Vec<String> = context.dev_relays.iter().map(|s| s.to_string()).collect();
        publish_relay_lists(&test_client, relay_urls).await?;

        test_client.disconnect().await;
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

        let account = context
            .whitenoise
            .login(known_keys.secret_key().to_secret_hex())
            .await?;

        assert_eq!(account.pubkey, known_pubkey);
        context.add_account("known_account", account);

        tracing::info!("✓ Logged in with known keys");
        Ok(())
    }
}
