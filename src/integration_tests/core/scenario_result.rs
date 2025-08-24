use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ScenarioResult {
    pub scenario_name: String,
    pub tests_run: u32,
    pub tests_passed: u32,
    pub tests_failed: u32,
    pub duration: Duration,
    pub success: bool,
}

impl ScenarioResult {
    pub fn new(name: &str, tests_run: u32, tests_passed: u32, duration: Duration) -> Self {
        Self {
            scenario_name: name.to_string(),
            tests_run,
            tests_passed,
            tests_failed: tests_run - tests_passed,
            duration,
            success: tests_passed == tests_run,
        }
    }

    pub fn failed(name: &str, tests_run: u32, tests_passed: u32, duration: Duration) -> Self {
        Self {
            scenario_name: name.to_string(),
            tests_run,
            tests_passed,
            tests_failed: tests_run - tests_passed,
            duration,
            success: false, // Explicitly mark as failed due to scenario-level error
        }
    }
}
