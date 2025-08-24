use crate::integration_tests::{
    core::*,
    test_cases::{shared::*, subscription_processing::*},
};
use crate::{Whitenoise, WhitenoiseError};
use async_trait::async_trait;
use nostr_sdk::prelude::*;

pub struct SubscriptionProcessingScenario {
    context: ScenarioContext,
}

impl SubscriptionProcessingScenario {
    pub fn new(whitenoise: &'static Whitenoise) -> Self {
        Self {
            context: ScenarioContext::new(whitenoise),
        }
    }
}

#[async_trait]
impl Scenario for SubscriptionProcessingScenario {
    fn context(&self) -> &ScenarioContext {
        &self.context
    }

    async fn run_scenario(&mut self) -> Result<(), WhitenoiseError> {
        CreateAccountsTestCase::with_names(vec!["subscription_test_user"])
            .execute(&mut self.context)
            .await?;

        // Test subscription-driven metadata updates
        let updated_metadata = Metadata {
            name: Some("Updated User via Subscription".to_string()),
            ..Default::default()
        };

        PublishMetadataUpdateTestCase::new("subscription_test_user", updated_metadata)
            .execute(&mut self.context)
            .await?;

        // Test subscription-driven relay list updates
        let new_relay_url = "wss://sub-update.example.com".to_string();
        PublishRelayListUpdateTestCase::new("subscription_test_user", new_relay_url)
            .execute(&mut self.context)
            .await?;

        Ok(())
    }
}
