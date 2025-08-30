use crate::integration_tests::{core::*, test_cases::user_discovery::*};
use crate::{Whitenoise, WhitenoiseError};
use async_trait::async_trait;

pub struct UserDiscoveryScenario {
    context: ScenarioContext,
}

impl UserDiscoveryScenario {
    pub fn new(whitenoise: &'static Whitenoise) -> Self {
        Self {
            context: ScenarioContext::new(whitenoise),
        }
    }
}

#[async_trait]
impl Scenario for UserDiscoveryScenario {
    fn context(&self) -> &ScenarioContext {
        &self.context
    }

    async fn run_scenario(&mut self) -> Result<(), WhitenoiseError> {
        tracing::info!("Testing: No metadata and no relays");
        FindOrCreateUserTestCase::basic()
            .execute(&mut self.context)
            .await?;

        tracing::info!("Testing: With metadata");
        FindOrCreateUserTestCase::basic()
            .with_metadata()
            .execute(&mut self.context)
            .await?;

        tracing::info!("Testing: With relays");
        FindOrCreateUserTestCase::basic()
            .with_relays()
            .execute(&mut self.context)
            .await?;

        tracing::info!("Testing: With metadata and relays");
        FindOrCreateUserTestCase::basic()
            .with_metadata()
            .with_relays()
            .execute(&mut self.context)
            .await?;

        Ok(())
    }
}
