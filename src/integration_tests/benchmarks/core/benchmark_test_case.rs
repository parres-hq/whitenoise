use std::time::Duration;

use async_trait::async_trait;

use crate::WhitenoiseError;
use crate::integration_tests::core::ScenarioContext;

/// Trait for atomic benchmark operations
#[async_trait]
pub trait BenchmarkTestCase {
    /// Run a single iteration of the benchmark operation and return the duration
    async fn run_iteration(
        &self,
        context: &mut ScenarioContext,
    ) -> Result<Duration, WhitenoiseError>;
}
