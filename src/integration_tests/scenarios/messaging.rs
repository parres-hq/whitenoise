use crate::integration_tests::{
    core::*,
    test_cases::{messaging::*, shared::*},
};
use crate::{Whitenoise, WhitenoiseError};
use async_trait::async_trait;

pub struct MessagingScenario {
    context: ScenarioContext,
}

impl MessagingScenario {
    pub fn new(whitenoise: &'static Whitenoise) -> Self {
        Self {
            context: ScenarioContext::new(whitenoise),
        }
    }
}

#[async_trait]
impl Scenario for MessagingScenario {
    fn context(&self) -> &ScenarioContext {
        &self.context
    }

    async fn run_scenario(&mut self) -> Result<(), WhitenoiseError> {
        CreateAccountsTestCase::with_names(vec!["creator", "account2"])
            .execute(&mut self.context)
            .await?;

        CreateGroupTestCase::basic()
            .with_name("Messaging Test Group")
            .with_members("creator", vec!["account2"])
            .execute(&mut self.context)
            .await?;

        SendMessageTestCase::basic()
            .execute(&mut self.context)
            .await?;

        let basic_message_id = self
            .context
            .get_message_id("basic_message")? // This is the default message id for the test case
            .clone();

        SendMessageTestCase::basic()
            .into_reaction("👍", &basic_message_id)
            .execute(&mut self.context)
            .await?;

        SendMessageTestCase::basic()
            .into_reply("Great message!", &basic_message_id)
            .execute(&mut self.context)
            .await?;

        Ok(())
    }
}
