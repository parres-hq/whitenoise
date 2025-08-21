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
        CreateAccountsTestCase::with_names(vec!["account1", "account2"])
            .execute(&mut self.context)
            .await?;

        LoginWithKnownKeysTestCase
            .execute(&mut self.context)
            .await?;

        Ok(())
    }
}
