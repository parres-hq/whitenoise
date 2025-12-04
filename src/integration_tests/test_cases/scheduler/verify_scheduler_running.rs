use crate::WhitenoiseError;
use crate::integration_tests::core::*;
use async_trait::async_trait;

/// Verifies scheduler lifecycle: running state and graceful shutdown.
pub struct SchedulerLifecycleTestCase;

#[async_trait]
impl TestCase for SchedulerLifecycleTestCase {
    async fn run(&self, context: &mut ScenarioContext) -> Result<(), WhitenoiseError> {
        // Verify scheduler is running
        tracing::info!("Verifying scheduler is running...");

        let task_count = context.whitenoise.scheduler_task_count().await;
        assert!(
            task_count > 0,
            "Scheduler should have at least one task running"
        );

        tracing::info!("✓ Scheduler is running with {} task(s)", task_count);

        // Verify shutdown completes gracefully
        tracing::info!("Verifying shutdown completes gracefully...");

        context.whitenoise.shutdown().await?;

        let task_count = context.whitenoise.scheduler_task_count().await;
        assert_eq!(
            task_count, 0,
            "Scheduler should have no tasks after shutdown"
        );

        tracing::info!("✓ Shutdown completed gracefully");

        Ok(())
    }
}
