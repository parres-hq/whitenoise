use crate::integration_tests::core::*;
use crate::WhitenoiseError;
use async_trait::async_trait;
use nostr_sdk::PublicKey;

pub struct UnfollowUserTestCase {
    follower_account_name: String,
    target_pubkey: PublicKey,
}

impl UnfollowUserTestCase {
    pub fn new(follower_account_name: &str, target_pubkey: PublicKey) -> Self {
        Self {
            follower_account_name: follower_account_name.to_string(),
            target_pubkey,
        }
    }
}

#[async_trait]
impl TestCase for UnfollowUserTestCase {
    async fn run(&self, context: &mut ScenarioContext) -> Result<(), WhitenoiseError> {
        tracing::info!(
            "Unfollowing user {} from account: {}",
            &self.target_pubkey.to_hex()[..8],
            self.follower_account_name
        );

        let account = context.get_account(&self.follower_account_name)?;

        // Perform the unfollow operation
        context
            .whitenoise
            .unfollow_user(account, &self.target_pubkey)
            .await?;

        // Add small delay for async operations
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let is_following = context
            .whitenoise
            .is_following_user(account, &self.target_pubkey)
            .await?;
        assert!(
            !is_following,
            "Account {} should not be following user {} after unfollow",
            self.follower_account_name,
            &self.target_pubkey.to_hex()[..8]
        );

        tracing::info!(
            "✓ Account {} is no longer following user {}",
            self.follower_account_name,
            &self.target_pubkey.to_hex()[..8]
        );

        Ok(())
    }
}
