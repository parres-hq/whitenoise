use crate::integration_tests::core::*;
use crate::WhitenoiseError;
use async_trait::async_trait;

pub struct CreateAccountsTestCase {
    account_names: Vec<String>,
}

impl CreateAccountsTestCase {
    pub fn with_names(account_names: Vec<&str>) -> Self {
        Self {
            account_names: account_names.iter().map(|s| s.to_string()).collect(),
        }
    }
}

#[async_trait]
impl TestCase for CreateAccountsTestCase {
    async fn run(&self, context: &mut ScenarioContext) -> Result<(), WhitenoiseError> {
        tracing::info!("Creating {} accounts...", self.account_names.len());

        let initial_count = context.whitenoise.get_accounts_count().await?;

        for name in &self.account_names {
            let account = context.whitenoise.create_identity().await?;
            tracing::info!("✓ Created {}: {}", name, account.pubkey.to_hex());
            context.add_account(name, account);
        }

        let final_count = context.whitenoise.get_accounts_count().await?;
        assert_eq!(final_count, initial_count + self.account_names.len());

        tracing::info!("✓ Created {} accounts", self.account_names.len());
        Ok(())
    }
}
