use crate::integration_tests::{
    core::*,
    test_cases::{follow_management::*, shared::*},
};
use crate::{Whitenoise, WhitenoiseError};
use async_trait::async_trait;
use nostr_sdk::Keys;

pub struct FollowManagementScenario {
    context: ScenarioContext,
}

impl FollowManagementScenario {
    pub fn new(whitenoise: &'static Whitenoise) -> Self {
        Self {
            context: ScenarioContext::new(whitenoise),
        }
    }
}

#[async_trait]
impl Scenario for FollowManagementScenario {
    fn context(&self) -> &ScenarioContext {
        &self.context
    }

    async fn run_scenario(&mut self) -> Result<(), WhitenoiseError> {
        // Create accounts for the test
        CreateAccountsTestCase::with_names(vec!["follow_mgmt_follower", "follow_mgmt_target"])
            .execute(&mut self.context)
            .await?;

        // Create some test contact public keys
        let test_contact1 = Keys::generate().public_key();
        let test_contact2 = Keys::generate().public_key();

        // Test following a single user
        FollowUserTestCase::new("follow_mgmt_follower", test_contact1)
            .execute(&mut self.context)
            .await?;

        // Test following a second user
        FollowUserTestCase::new("follow_mgmt_follower", test_contact2)
            .execute(&mut self.context)
            .await?;

        // Test unfollowing the first user
        UnfollowUserTestCase::new("follow_mgmt_follower", test_contact1)
            .execute(&mut self.context)
            .await?;

        // Test error handling: try to follow the same user again (should succeed without error)
        FollowUserTestCase::new("follow_mgmt_follower", test_contact2)
            .execute(&mut self.context)
            .await?;

        // Test error handling: try to unfollow a non-existent follow relationship
        let non_existent_contact = Keys::generate().public_key();
        UnfollowUserTestCase::new("follow_mgmt_follower", non_existent_contact)
            .execute(&mut self.context)
            .await?;

        // Test following an existing account (cross-account following)
        let target_account = self.context.get_account("follow_mgmt_target")?;
        FollowUserTestCase::new("follow_mgmt_follower", target_account.pubkey)
            .execute(&mut self.context)
            .await?;

        Ok(())
    }
}
