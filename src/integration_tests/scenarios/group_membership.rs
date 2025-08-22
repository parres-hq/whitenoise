use crate::integration_tests::{
    core::*,
    test_cases::{group_membership::*, shared::*},
};
use crate::{Whitenoise, WhitenoiseError};
use async_trait::async_trait;
use nostr_sdk::Keys;

pub struct GroupMembershipScenario {
    context: ScenarioContext,
}

impl GroupMembershipScenario {
    pub fn new(whitenoise: &'static Whitenoise) -> Self {
        Self {
            context: ScenarioContext::new(whitenoise),
        }
    }
}

#[async_trait]
impl Scenario for GroupMembershipScenario {
    fn context(&self) -> &ScenarioContext {
        &self.context
    }

    async fn run_scenario(&mut self) -> Result<(), WhitenoiseError> {
        // Create accounts for testing group membership
        CreateAccountsTestCase::with_names(vec!["admin_account", "member1", "member2"])
            .execute(&mut self.context)
            .await?;

        // Create a test group with admin and initial member
        CreateGroupTestCase::basic()
            .with_members("admin_account", vec!["member1"])
            .execute(&mut self.context)
            .await?;

        // Get the created group from context and clone the group_id
        let group_id = {
            let test_group = self.context.get_group("test_group")?;
            test_group.mls_group_id.clone()
        };

        // Get account pubkeys before mutable operations
        let member2_pubkey = self.context.get_account("member2")?.pubkey;

        // Test adding a single member
        AddGroupMembersTestCase::new("admin_account", group_id.clone(), vec![member2_pubkey])
            .execute(&mut self.context)
            .await?;

        // Create additional accounts for bulk member addition
        CreateAccountsTestCase::with_names(vec!["member3", "member4"])
            .execute(&mut self.context)
            .await?;

        let member3_pubkey = self.context.get_account("member3")?.pubkey;
        let member4_pubkey = self.context.get_account("member4")?.pubkey;

        // Test adding multiple members at once
        AddGroupMembersTestCase::new(
            "admin_account",
            group_id.clone(),
            vec![member3_pubkey, member4_pubkey],
        )
        .execute(&mut self.context)
        .await?;

        // Test removing a single member
        RemoveGroupMembersTestCase::new("admin_account", group_id.clone(), vec![member2_pubkey])
            .execute(&mut self.context)
            .await?;

        // Test error handling: try to add a member that doesn't have key packages
        let no_keypackage_user = Keys::generate().public_key();
        AddGroupMembersTestCase::new("admin_account", group_id.clone(), vec![no_keypackage_user])
            .expect_failure() // This should fail because user doesn't have key packages
            .execute(&mut self.context)
            .await?;

        // Test error handling: try to remove a member that's not in the group
        let non_member_user = Keys::generate().public_key();
        RemoveGroupMembersTestCase::new("admin_account", group_id, vec![non_member_user])
            .expect_failure() // This should fail because user is not in the group
            .execute(&mut self.context)
            .await?;

        Ok(())
    }
}
