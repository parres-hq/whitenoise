use crate::integration_tests::core::*;
use crate::{PublicKey, WhitenoiseError};
use async_trait::async_trait;
use nostr_mls::prelude::GroupId;

pub struct AddGroupMembersTestCase {
    admin_account_name: String,
    group_id: GroupId,
    new_member_pubkeys: Vec<PublicKey>,
    expect_failure: bool,
}

impl AddGroupMembersTestCase {
    pub fn new(
        admin_account_name: &str,
        group_id: GroupId,
        new_member_pubkeys: Vec<PublicKey>,
    ) -> Self {
        Self {
            admin_account_name: admin_account_name.to_string(),
            group_id,
            new_member_pubkeys,
            expect_failure: false,
        }
    }

    pub fn expect_failure(mut self) -> Self {
        self.expect_failure = true;
        self
    }
}

#[async_trait]
impl TestCase for AddGroupMembersTestCase {
    async fn run(&self, context: &mut ScenarioContext) -> Result<(), WhitenoiseError> {
        tracing::info!(
            "Adding {} members to group from admin account: {}",
            self.new_member_pubkeys.len(),
            self.admin_account_name
        );

        let admin_account = context.get_account(&self.admin_account_name)?;

        // Get initial member count for verification
        let initial_members = context
            .whitenoise
            .fetch_group_members(admin_account, &self.group_id)
            .await?;

        // Add members to the group
        let add_result = context
            .whitenoise
            .add_members_to_group(
                admin_account,
                &self.group_id,
                self.new_member_pubkeys.clone(),
            )
            .await;

        // Handle expected failure vs success cases
        if self.expect_failure {
            match add_result {
                Ok(_) => {
                    return Err(WhitenoiseError::Other(anyhow::anyhow!(
                        "Expected adding {} members to fail, but it succeeded",
                        self.new_member_pubkeys.len()
                    )));
                }
                Err(e) => {
                    tracing::info!(
                        "✓ Adding {} members failed as expected: {}",
                        self.new_member_pubkeys.len(),
                        e
                    );
                    return Ok(());
                }
            }
        } else {
            add_result?;
        }

        // Wait for MLS processing and event propagation
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let updated_members = context
            .whitenoise
            .fetch_group_members(admin_account, &self.group_id)
            .await?;
        let expected_count = initial_members.len() + self.new_member_pubkeys.len();

        assert_eq!(
            updated_members.len(),
            expected_count,
            "Expected {} members after addition, found {}",
            expected_count,
            updated_members.len()
        );

        // Verify each new member is in the group
        for pubkey in &self.new_member_pubkeys {
            assert!(
                updated_members.contains(pubkey),
                "New member {} should be in the group after addition",
                &pubkey.to_hex()[..8]
            );
        }

        tracing::info!(
            "✓ All {} new members verified in group (total members: {})",
            self.new_member_pubkeys.len(),
            updated_members.len()
        );

        Ok(())
    }
}
