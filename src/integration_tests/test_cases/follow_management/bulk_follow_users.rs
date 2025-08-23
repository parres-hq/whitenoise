use crate::integration_tests::core::*;
use crate::{PublicKey, WhitenoiseError};
use async_trait::async_trait;

pub struct BulkFollowUsersTestCase {
    follower_account_name: String,
    target_pubkeys: Vec<PublicKey>,
}

impl BulkFollowUsersTestCase {
    pub fn new(follower_account_name: &str, target_pubkeys: Vec<PublicKey>) -> Self {
        Self {
            follower_account_name: follower_account_name.to_string(),
            target_pubkeys,
        }
    }
}

#[async_trait]
impl TestCase for BulkFollowUsersTestCase {
    async fn run(&self, context: &mut ScenarioContext) -> Result<(), WhitenoiseError> {
        tracing::info!(
            "Bulk following {} users from account: {}",
            self.target_pubkeys.len(),
            self.follower_account_name
        );

        let account = context.get_account(&self.follower_account_name)?;

        // Perform the bulk follow operation
        context
            .whitenoise
            .follow_users(account, &self.target_pubkeys)
            .await?;

        // Add small delay for async operations
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        for pubkey in &self.target_pubkeys {
            let is_following = context
                .whitenoise
                .is_following_user(account, pubkey)
                .await?;
            assert!(
                is_following,
                "Account {} should be following user {} after bulk follow",
                self.follower_account_name,
                &pubkey.to_hex()[..8]
            );
        }

        tracing::info!(
            "✓ Account {} is now following {} users via bulk operation",
            self.follower_account_name,
            self.target_pubkeys.len()
        );

        Ok(())
    }
}
