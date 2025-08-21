use crate::integration_tests::core::*;
use crate::WhitenoiseError;
use async_trait::async_trait;

pub struct LogoutAccountTestCase {
    account_name: String,
    expected_remaining_accounts: Vec<String>,
}

impl LogoutAccountTestCase {
    pub fn for_account(account_name: &str) -> Self {
        Self {
            account_name: account_name.to_string(),
            expected_remaining_accounts: Vec::new(),
        }
    }

    pub fn expect_remaining_accounts(mut self, account_names: Vec<&str>) -> Self {
        self.expected_remaining_accounts = account_names.iter().map(|s| s.to_string()).collect();
        self
    }
}

#[async_trait]
impl TestCase for LogoutAccountTestCase {
    async fn run(&self, context: &mut ScenarioContext) -> Result<(), WhitenoiseError> {
        tracing::info!("Logging out account: {}", self.account_name);

        let account = context.get_account(&self.account_name)?;
        let initial_count = context.whitenoise.get_accounts_count().await?;

        // Perform logout
        context.whitenoise.logout(&account.pubkey).await?;

        // Verify the account count decreased
        let final_count = context.whitenoise.get_accounts_count().await?;
        assert_eq!(
            final_count,
            initial_count - 1,
            "Account count should decrease by 1 after logout"
        );

        // Verify specific accounts remain if specified
        if !self.expected_remaining_accounts.is_empty() {
            let final_accounts = context.whitenoise.all_accounts().await?;

            for expected_name in &self.expected_remaining_accounts {
                let account_exists = context.accounts.contains_key(expected_name);
                assert!(
                    account_exists,
                    "Expected account '{}' should still exist in context after logout",
                    expected_name
                );

                let account = context.get_account(expected_name)?;
                let exists_in_db = final_accounts.iter().any(|a| a.pubkey == account.pubkey);
                assert!(
                    exists_in_db,
                    "Expected account '{}' should exist in database after logout",
                    expected_name
                );
            }

            tracing::info!(
                "✓ Account {} logged out successfully. Remaining accounts: {} (verified: {:?})",
                self.account_name,
                final_count,
                self.expected_remaining_accounts
            );
        } else {
            tracing::info!(
                "✓ Account {} logged out successfully. Remaining accounts: {}",
                self.account_name,
                final_count
            );
        }

        Ok(())
    }
}
