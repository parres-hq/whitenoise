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
        CreateAccountsTestCase::with_names(vec!["follower_user", "account2"])
            .execute(&mut self.context)
            .await?;

        // Create some test contact public keys
        let test_contact1 = Keys::generate().public_key();
        let test_contact2 = Keys::generate().public_key();
        let test_contact3 = Keys::generate().public_key();

        // Test following a single user
        FollowUserTestCase::new("follower_user", test_contact1)
            .execute(&mut self.context)
            .await?;

        // Test following a second user
        FollowUserTestCase::new("follower_user", test_contact2)
            .execute(&mut self.context)
            .await?;

        // Test unfollowing the first user
        UnfollowUserTestCase::new("follower_user", test_contact1)
            .execute(&mut self.context)
            .await?;

        // Test bulk following multiple users
        let bulk_contacts = vec![test_contact2, test_contact3];
        BulkFollowUsersTestCase::new("follower_user", bulk_contacts)
            .execute(&mut self.context)
            .await?;

        // Test error handling: try to follow the same user again (should succeed without error)
        FollowUserTestCase::new("follower_user", test_contact2)
            .execute(&mut self.context)
            .await?;

        // Test error handling: try to unfollow a non-existent follow relationship
        let non_existent_contact = Keys::generate().public_key();
        UnfollowUserTestCase::new("follower_user", non_existent_contact)
            .execute(&mut self.context)
            .await?;

        // Test following an existing account (cross-account following)
        let account2 = self.context.get_account("account2")?;
        FollowUserTestCase::new("follower_user", account2.pubkey)
            .execute(&mut self.context)
            .await?;

        Ok(())
    }
}
