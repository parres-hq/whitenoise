use crate::integration_tests::core::*;
use crate::WhitenoiseError;
use async_trait::async_trait;

pub struct AcceptGroupInviteTestCase {
    account_name: String,
}

impl AcceptGroupInviteTestCase {
    pub fn new(account_name: &str) -> Self {
        Self {
            account_name: account_name.to_string(),
        }
    }
}

#[async_trait]
impl TestCase for AcceptGroupInviteTestCase {
    async fn run(&self, context: &mut ScenarioContext) -> Result<(), WhitenoiseError> {
        tracing::info!(
            "Accepting group invitations for account '{}'...",
            self.account_name
        );

        let account = context.get_account(&self.account_name)?;

        // Fetch pending welcome invitations
        let welcomes = context.whitenoise.pending_welcomes(&account.pubkey).await?;
        let welcome_count = welcomes.len();

        tracing::info!("Found {} pending welcome invitations", welcome_count);

        // Accept all pending welcomes
        for welcome in welcomes {
            let welcome_id = welcome.id.to_string();
            tracing::info!("Accepting welcome invitation with ID: {}", welcome_id);

            context
                .whitenoise
                .accept_welcome(&account.pubkey, welcome_id)
                .await?;

            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        // Verify that the account now has access to groups
        let groups = context.whitenoise.groups(account, true).await?;
        tracing::info!(
            "Account '{}' now has access to {} groups",
            self.account_name,
            groups.len()
        );

        // Assert that the account has access to at least one group after accepting invitations
        if welcome_count > 0 {
            assert!(
                !groups.is_empty(),
                "Account '{}' should have access to groups after accepting {} invitations",
                self.account_name,
                welcome_count
            );
        }

        tracing::info!(
            "✓ All group invitations accepted for '{}' - verified access to {} groups",
            self.account_name,
            groups.len()
        );
        Ok(())
    }
}
