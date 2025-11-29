use std::time::Duration;

use async_trait::async_trait;

use super::Whitenoise;
use crate::WhitenoiseError;

/// Trait for implementing scheduled background tasks.
///
/// Tasks are executed on startup and periodically thereafter based on their configured interval.
/// Implementations should be idempotent and handle transient failures gracefully.
// TODO: Remove allow(dead_code) once scheduler is wired up
#[allow(dead_code)]
#[async_trait]
pub trait Task: Send + Sync {
    /// Returns the unique name of this task for logging and identification.
    fn name(&self) -> &'static str;

    /// Returns the interval between task executions.
    fn interval(&self) -> Duration;

    /// Executes the task.
    ///
    /// Implementations should:
    /// - Be idempotent (safe to run multiple times)
    /// - Handle transient failures gracefully (log and continue)
    /// - Avoid holding locks for extended periods
    async fn execute(&self, whitenoise: &'static Whitenoise) -> Result<(), WhitenoiseError>;
}

// TODO: Remove allow(dead_code) once scheduler is wired up
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    pub enabled: bool,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}
