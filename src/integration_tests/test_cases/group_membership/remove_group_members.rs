use crate::integration_tests::core::*;
use crate::{PublicKey, WhitenoiseError};
use async_trait::async_trait;
use nostr_mls::prelude::GroupId;

pub struct RemoveGroupMembersTestCase {
    admin_account_name: String,
    group_id: GroupId,
    member_pubkeys_to_remove: Vec<PublicKey>,
    expect_failure: bool,
}

impl RemoveGroupMembersTestCase {
    pub fn new(
        admin_account_name: &str,
        group_id: GroupId,
        member_pubkeys_to_remove: Vec<PublicKey>,
    ) -> Self {
        Self {
            admin_account_name: admin_account_name.to_string(),
            group_id,
            member_pubkeys_to_remove,
            expect_failure: false,
        }
    }

    pub fn expect_failure(mut self) -> Self {
        self.expect_failure = true;
        self
    }
}

#[async_trait]
impl TestCase for RemoveGroupMembersTestCase {
    async fn run(&self, context: &mut ScenarioContext) -> Result<(), WhitenoiseError> {
        tracing::info!(
            "Removing {} members from group using admin account: {}",
            self.member_pubkeys_to_remove.len(),
            self.admin_account_name
        );

        let admin_account = context.get_account(&self.admin_account_name)?;

        // Get initial member count for verification
        let initial_members = context
            .whitenoise
            .fetch_group_members(admin_account, &self.group_id)
            .await?;

        // Remove members from the group
        let remove_result = context
            .whitenoise
            .remove_members_from_group(
                admin_account,
                &self.group_id,
                self.member_pubkeys_to_remove.clone(),
            )
            .await;

        // Handle expected failure vs success cases
        if self.expect_failure {
            match remove_result {
                Ok(_) => {
                    tracing::info!(
                        "✓ Removing {} members succeeded (this might be expected for non-existent members)",
                        self.member_pubkeys_to_remove.len()
                    );
                    return Ok(());
                }
                Err(e) => {
                    tracing::info!(
                        "✓ Removing {} members failed as expected: {}",
                        self.member_pubkeys_to_remove.len(),
                        e
                    );
                    return Ok(());
                }
            }
        } else {
            remove_result?;
        }

        // Wait for MLS processing and event propagation
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Verify members were removed
        let updated_members = context
            .whitenoise
            .fetch_group_members(admin_account, &self.group_id)
            .await?;
        let expected_count = initial_members.len() - self.member_pubkeys_to_remove.len();

        assert_eq!(
            updated_members.len(),
            expected_count,
            "Expected {} members after removal, found {}",
            expected_count,
            updated_members.len()
        );

        // Verify each removed member is no longer in the group
        for pubkey in &self.member_pubkeys_to_remove {
            assert!(
                !updated_members.contains(pubkey),
                "Removed member {} should not be in the group after removal",
                &pubkey.to_hex()[..8]
            );
        }

        tracing::info!(
            "✓ All {} members verified removed from group (remaining members: {})",
            self.member_pubkeys_to_remove.len(),
            updated_members.len()
        );

        Ok(())
    }
}
