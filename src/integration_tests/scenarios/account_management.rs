use crate::integration_tests::{
    core::*,
    test_cases::{account_management::*, shared::*},
};
use crate::{Whitenoise, WhitenoiseError};
use async_trait::async_trait;

pub struct AccountManagementScenario {
    context: ScenarioContext,
}

impl AccountManagementScenario {
    pub fn new(whitenoise: &'static Whitenoise) -> Self {
        Self {
            context: ScenarioContext::new(whitenoise),
        }
    }
}

#[async_trait]
impl Scenario for AccountManagementScenario {
    fn context(&self) -> &ScenarioContext {
        &self.context
    }

    async fn run_scenario(&mut self) -> Result<(), WhitenoiseError> {
        CreateAccountsTestCase::with_names(vec!["acct_mgmt_account1", "acct_mgmt_account2", "acct_mgmt_account3"])
            .execute(&mut self.context)
            .await?;

        LoginWithKnownKeysTestCase
            .execute(&mut self.context)
            .await?;

        // Test logout functionality with verification
        LogoutAccountTestCase::for_account("acct_mgmt_account2")
            .expect_remaining_accounts(vec!["acct_mgmt_account1", "acct_mgmt_account3"])
            .execute(&mut self.context)
            .await?;

        Ok(())
    }
}
