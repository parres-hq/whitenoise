use crate::integration_tests::core::{ScenarioContext, ScenarioResult};
use crate::WhitenoiseError;
use async_trait::async_trait;
use std::time::Instant;

#[async_trait]
pub trait TestCase {
    async fn run(&self, context: &mut ScenarioContext) -> Result<(), WhitenoiseError>;

    async fn execute(&self, context: &mut ScenarioContext) -> Result<(), WhitenoiseError> {
        let result = self.run(context).await;
        context.record_test(result.is_ok());
        result
    }
}

#[async_trait]
pub trait Scenario {
    /// Get the name of this scenario for logging and reporting
    fn scenario_name(&self) -> &'static str {
        std::any::type_name::<Self>()
            .rsplit("::")
            .next()
            .unwrap_or(std::any::type_name::<Self>())
    }

    /// Get immutable access to the scenario's context
    fn context(&self) -> &ScenarioContext;

    /// Run the actual scenario logic - implement this in each scenario
    async fn run_scenario(&mut self) -> Result<(), WhitenoiseError>;

    /// Execute the scenario with consistent timing, logging and error handling
    /// Always returns a ScenarioResult to ensure consistent reporting
    async fn execute(mut self) -> (ScenarioResult, Option<WhitenoiseError>)
    where
        Self: Sized,
    {
        let start_time = Instant::now();
        let scenario_name = self.scenario_name();

        tracing::info!("=== Running Scenario: {} ===", scenario_name);

        let run_result = self.run_scenario().await;
        let duration = start_time.elapsed();

        let context = self.context();
        let tests_run = context.tests_count;
        let tests_passed = context.tests_passed;

        match run_result {
            Ok(()) => {
                tracing::info!(
                    "✓ {} Scenario completed ({}/{}) in {:?}",
                    scenario_name,
                    tests_passed,
                    tests_run,
                    duration
                );

                let cleanup_result = self.cleanup().await;
                if cleanup_result.is_err() {
                    tracing::error!(
                        "✗ {} Scenario cleanup failed: {}",
                        scenario_name,
                        cleanup_result.err().unwrap()
                    );
                }

                (
                    ScenarioResult::new(scenario_name, tests_run, tests_passed, duration),
                    None,
                )
            }
            Err(e) => {
                tracing::error!(
                    "✗ {} Scenario failed after {} completed tests in {:?}: {}",
                    scenario_name,
                    tests_passed,
                    duration,
                    e
                );

                (
                    ScenarioResult::failed(scenario_name, tests_run, tests_passed, duration),
                    Some(e),
                )
            }
        }
    }

    async fn cleanup(&mut self) -> Result<(), WhitenoiseError> {
        let context = self.context();
        for account in context.accounts.values() {
            if let Err(e) = context.whitenoise.logout(&account.pubkey).await {
                match e {
                    WhitenoiseError::AccountNotFound => {} // Account already logged out
                    _ => return Err(e),
                }
            }
        }

        context.whitenoise.wipe_database().await?;
        context.whitenoise.reset_nostr_client().await?;

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        Ok(())
    }
}
