use crate::integration_tests::core::*;
use crate::WhitenoiseError;
use async_trait::async_trait;
use nostr_sdk::PublicKey;

pub struct FollowUserTestCase {
    follower_account_name: String,
    target_pubkey: PublicKey,
}

impl FollowUserTestCase {
    pub fn new(follower_account_name: &str, target_pubkey: PublicKey) -> Self {
        Self {
            follower_account_name: follower_account_name.to_string(),
            target_pubkey,
        }
    }
}

#[async_trait]
impl TestCase for FollowUserTestCase {
    async fn run(&self, context: &mut ScenarioContext) -> Result<(), WhitenoiseError> {
        tracing::info!(
            "Following user {} from account: {}",
            &self.target_pubkey.to_hex()[..8],
            self.follower_account_name
        );

        let account = context.get_account(&self.follower_account_name)?;

        // Perform the follow operation
        context
            .whitenoise
            .follow_user(account, &self.target_pubkey)
            .await?;

        // Add small delay for async operations
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        let is_following = context
            .whitenoise
            .is_following_user(account, &self.target_pubkey)
            .await?;
        assert!(
            is_following,
            "Account {} should be following user {}",
            self.follower_account_name,
            &self.target_pubkey.to_hex()[..8]
        );

        tracing::info!(
            "✓ Account {} is now following user {}",
            self.follower_account_name,
            &self.target_pubkey.to_hex()[..8]
        );

        Ok(())
    }
}
