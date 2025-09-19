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

        // Wait for follow operation to be reflected in the system
        retry_default(
            || async {
                let is_following = context
                    .whitenoise
                    .is_following_user(account, &self.target_pubkey)
                    .await?;

                if is_following {
                    Ok(())
                } else {
                    Err(WhitenoiseError::Other(anyhow::anyhow!(
                        "Follow relationship not yet established"
                    )))
                }
            },
            &format!(
                "verify follow relationship for user {}",
                &self.target_pubkey.to_hex()[..8]
            ),
        )
        .await?;

        tracing::info!(
            "✓ Account {} is now following user {}",
            self.follower_account_name,
            &self.target_pubkey.to_hex()[..8]
        );

        Ok(())
    }
}
