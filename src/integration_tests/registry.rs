use crate::integration_tests::core::*;
use crate::integration_tests::scenarios::*;
use crate::{Whitenoise, WhitenoiseError};
use std::time::{Duration, Instant};

pub struct ScenarioRegistry;

impl ScenarioRegistry {
    pub async fn run_all_scenarios(whitenoise: &'static Whitenoise) -> Result<(), WhitenoiseError> {
        let overall_start = Instant::now();
        let mut results = Vec::new();
        let mut first_error = None;

        macro_rules! run_scenario {
            ($scenario_type:ty) => {
                let (result, error) = <$scenario_type>::new(whitenoise).execute().await;
                results.push(result);
                if error.is_some() && first_error.is_none() {
                    first_error = error;
                }
            };
        }

        run_scenario!(AccountManagementScenario);
        run_scenario!(AppSettingsScenario);
        run_scenario!(MetadataManagementScenario);
        run_scenario!(BasicMessagingScenario);
        run_scenario!(FollowManagementScenario);
        run_scenario!(AdvancedMessagingScenario);
        run_scenario!(GroupMembershipScenario);

        Self::print_summary(&results, overall_start.elapsed()).await;

        // Return the first error encountered, if any
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    async fn print_summary(results: &[ScenarioResult], overall_duration: Duration) {
        tokio::time::sleep(Duration::from_secs(1)).await; // Wait for the logs to be flushed
        tracing::info!("=== Integration Test Summary ===");
        tracing::info!("Total duration: {:?}", overall_duration);

        let total_passed = results.iter().map(|r| r.tests_passed).sum::<u32>();
        let total_failed = results.iter().map(|r| r.tests_failed).sum::<u32>();

        let scenarios_passed = results.iter().filter(|r| r.success).count();
        let scenarios_failed = results.iter().filter(|r| !r.success).count();

        tracing::info!(
            "Scenarios: {} passed, {} failed",
            scenarios_passed,
            scenarios_failed
        );
        tracing::info!(
            "Test Cases: {} passed, {} failed",
            total_passed,
            total_failed
        );

        tracing::info!("Detailed Results:");
        for result in results {
            let status = if result.success { "✓" } else { "✗" };
            tracing::info!(
                "  {} {} - {}/{} tests passed in {:?}",
                status,
                result.scenario_name,
                result.tests_passed,
                result.tests_run,
                result.duration
            );
        }

        // Give async logging time to flush before program exits
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}
