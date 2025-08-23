# Integration Tests

This directory contains the modular integration test framework for Whitenoise, designed for maintainability, reusability, and comprehensive testing coverage.

## Architecture Principles

### 🎯 **Scenarios** - High-Level Test Workflows

Scenarios orchestrate multiple TestCases to test complete user workflows or system behaviors.
Each scenario is designed to be completely independent, with fresh context and cleanup between runs.
That said, given the nature of the singleton Whitenoise instance, there might be some leakage between scenarios.

**Responsibilities:**

- Define the high-level test flow (e.g., "test complete messaging workflow")
- Coordinate multiple TestCases in the correct order
- Provide scenario-specific context and setup
- Handle scenario-level error handling and cleanup

**Design Rules:**

- **Independence**: Each scenario is completely self-contained
- **Fresh Context**: Each scenario gets a new `ScenarioContext`
- **No Cross-Contamination**: No data sharing between scenarios

### 🔧 **TestCases** - Atomic Test Operations

TestCases perform single, focused operations that can be reused across scenarios.

**Responsibilities:**

- Test one specific functionality (e.g., "send a message", "create an account")
- Mutate the shared context to pass data to subsequent TestCases
- Provide meaningful assertions and error messages
- Handle operation-specific timing and setup

**Design Rules:**

- **Single Responsibility**: One TestCase = One logical operation
- **Builder Pattern**: Use fluent builders for configuration
- **Context Mutation**: Store results in context for other TestCases to use
- **Reusability**: Design for use in multiple scenarios

### 📦 **ScenarioContext** - Data Passing Between TestCases

ScenarioContext enables TestCases within the same scenario to share data.

**Allowed Usage:**

- Store accounts, groups, and message IDs created during the scenario
  - We can add more items, but be mindful to avoid unnecessary bloat
- Pass data between TestCases in the same scenario
- Track test execution statistics

**Forbidden Usage:**

- Cross-scenario data sharing
- Long-lived caches or state mirrors
- Complex business logic

## Directory Structure

```text
src/integration_tests/
├── mod.rs                    # Main module exports
├── registry.rs               # ScenarioRegistry - runs all scenarios
├── core/                     # Core framework components
│   ├── mod.rs
│   ├── context.rs           # ScenarioContext for data passing
│   ├── scenario_result.rs   # Test result tracking
│   ├── test_clients.rs      # Test client utilities
│   └── traits.rs            # Scenario and TestCase traits
├── scenarios/                # High-level test workflows
│   ├── mod.rs
│   ├── account_management.rs # Account creation, login, logout
│   ├── basic_messaging.rs   # Simple messaging workflows
│   └── ...                  # Additional scenario files
└── test_cases/              # Reusable atomic test operations
    ├── mod.rs
    ├── account_management/   # Account-specific operations
    │   ├── mod.rs
    │   ├── login.rs
    │   └── logout_account.rs
    ├── messaging/            # Message-specific operations
    │   ├── mod.rs
    │   └── send_message.rs
    ├── shared/              # Cross-scenario reusable operations
    │   ├── mod.rs
    │   ├── create_accounts.rs
    │   └── create_group.rs
    └── ...                  # Additional test case directories
```

## Adding New Tests

### Adding a New TestCase

1. **Choose the Right Directory:**

   - `test_cases/shared/` - Reusable across multiple scenarios
   - `test_cases/{domain}/` - Specific to one domain (messaging, account_management, etc.)

2. **Follow the TestCase Pattern:**

   ```rust
   use crate::integration_tests::core::*;
   use crate::WhitenoiseError;
   use async_trait::async_trait;

   // 1. Define struct with configuration
   pub struct YourTestCase {
       // Configuration fields
   }

   // 2. Implement builder methods (only if needed)
   impl YourTestCase {
       pub fn new(/* params */) -> Self { /* */ }
       pub fn with_option(mut self, option: SomeType) -> Self { /* */ }
   }

   // 3. Implement TestCase trait
   #[async_trait]
   impl TestCase for YourTestCase {
       async fn run(&self, context: &mut ScenarioContext) -> Result<(), WhitenoiseError> {
           // 1. Get data from context
           // 2. Perform operation
           // 3. Store results in context
           // 4. Add assertions
           Ok(())
       }
   }
   ```

3. **Update Module Exports:**

   ```rust
   // In the appropriate mod.rs file
   pub mod your_test_case;
   pub use your_test_case::*;
   ```

### Adding a New Scenario

1. **Create the Scenario File:**

   ```bash
   touch src/integration_tests/scenarios/your_scenario.rs
   ```

2. **Follow the Scenario Pattern:**

   ```rust
   use crate::integration_tests::core::*;
   use crate::{Whitenoise, WhitenoiseError};
   use async_trait::async_trait;

   pub struct YourScenario {
       context: ScenarioContext,
   }

   impl YourScenario {
       pub fn new(whitenoise: &'static Whitenoise) -> Self {
           Self {
               context: ScenarioContext::new(whitenoise),
           }
       }
   }

   #[async_trait]
   impl Scenario for YourScenario {
       fn context(&self) -> &ScenarioContext {
           &self.context
       }

       async fn run_scenario(&mut self) -> Result<(), WhitenoiseError> {
           // Compose TestCases to create your workflow
           // Note: Use .execute instead of .run inside a scenario.

           YourTestCase::new(/* params */)
               .execute(&mut self.context)
               .await?;

           Ok(())
       }
   }
   ```

3. **Update Module Exports:**

   ```rust
   // In scenarios/mod.rs
   pub mod your_scenario;
   pub use your_scenario::*;
   ```

4. **Add to ScenarioRegistry:**

   ```rust
   // In registry.rs
   impl ScenarioRegistry {
       pub async fn run_all_scenarios(whitenoise: &'static Whitenoise) -> Result<(), WhitenoiseError> {
           // ... existing scenarios ...
           run_scenario!(YourScenario);

           Self::print_summary(&results, overall_start.elapsed()).await;
           // ...
       }
   }
   ```
