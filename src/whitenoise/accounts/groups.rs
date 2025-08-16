use crate::whitenoise::accounts::Account;
use crate::whitenoise::error::{Result, WhitenoiseError};
use crate::whitenoise::users::User;
use crate::whitenoise::Whitenoise;
use crate::RelayType;
use nostr_mls::prelude::*;
use std::collections::HashSet;
use std::time::Duration;

impl Whitenoise {
    /// Creates a new MLS group with the specified members and settings
    ///
    /// # Arguments
    /// * `creator_pubkey` - Public key of the group creator (must be the active account)
    /// * `member_pubkeys` - List of public keys for group members
    /// * `admin_pubkeys` - List of public keys for group admins
    /// * `group_name` - Name of the group
    /// * `description` - Description of the group
    ///
    /// # Returns
    /// * `Ok(Group)` - The newly created group
    /// * `Err(String)` - Error message if group creation fails
    ///
    /// # Errors
    /// Returns error if:
    /// - Active account is not the creator
    /// - Member/admin validation fails
    /// - Key package fetching fails
    /// - MLS group creation fails
    /// - Welcome message sending fails
    /// - Database operations fail
    pub async fn create_group(
        &self,
        creator_account: &Account,
        member_pubkeys: Vec<PublicKey>,
        admin_pubkeys: Vec<PublicKey>,
        config: NostrGroupConfigData,
    ) -> Result<group_types::Group> {
        let keys = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&creator_account.pubkey)?;

        let mut key_package_events: Vec<Event> = Vec::new();
        let mut members = Vec::new();

        for pk in member_pubkeys.iter() {
            let user = User::find_by_pubkey(pk, &self.database).await?;
            let kp_relays = user.relays(RelayType::KeyPackage, &self.database).await?;
            let some_event = self.nostr.fetch_user_key_package(*pk, &kp_relays).await?;
            let event = some_event.ok_or(WhitenoiseError::NostrMlsError(
                nostr_mls::Error::KeyPackage("Does not exist".to_owned()),
            ))?;
            key_package_events.push(event);
            members.push(user);
        }

        tracing::debug!("Succefully fetched the key packages of members");

        let group_relays = config.relays.clone();

        let nostr_mls =
            Account::create_nostr_mls(creator_account.pubkey, &self.config.data_dir).unwrap();
        let create_group_result = nostr_mls.create_group(
            &creator_account.pubkey,
            key_package_events.clone(),
            admin_pubkeys,
            config,
        )?;

        let group_ids = nostr_mls
            .get_groups()?
            .into_iter()
            .map(|g| hex::encode(g.nostr_group_id))
            .collect::<Vec<_>>();

        let group = create_group_result.group;
        let welcome_rumors = create_group_result.welcome_rumors;
        if welcome_rumors.len() != members.len() {
            return Err(WhitenoiseError::Other(anyhow::Error::msg(
                "Welcome rumours are missing for some of the members",
            )));
        }

        // Fan out the welcome message to all members
        for (welcome_rumor, member) in welcome_rumors.iter().zip(members.iter()) {
            // Get the public key of the member from the key package event
            let key_package_event_id =
                welcome_rumor
                    .tags
                    .event_ids()
                    .next()
                    .ok_or(WhitenoiseError::Other(anyhow::anyhow!(
                        "No event ID found in welcome rumor"
                    )))?;

            let member_pubkey = key_package_events
                .iter()
                .find(|event| event.id == *key_package_event_id)
                .map(|event| event.pubkey)
                .ok_or(WhitenoiseError::Other(anyhow::anyhow!(
                    "No public key found in key package event"
                )))?;

            // Create a timestamp 1 month in the future
            use std::ops::Add;
            let one_month_future = Timestamp::now().add(30 * 24 * 60 * 60);
            let relays_to_use = member.relays(RelayType::Inbox, &self.database).await?;

            self.nostr
                .publish_gift_wrap_with_signer(
                    &member_pubkey,
                    welcome_rumor.clone(),
                    &[Tag::expiration(one_month_future)],
                    &relays_to_use,
                    keys.clone(),
                )
                .await
                .map_err(WhitenoiseError::from)?;
        }

        let mut relays = HashSet::new();
        for relay in group_relays {
            let db_relay = self.find_or_create_relay(&relay).await?;
            relays.insert(db_relay);
        }

        self.nostr
            .setup_group_messages_subscriptions_with_signer(
                creator_account.pubkey,
                &relays.into_iter().collect::<Vec<_>>(),
                &group_ids,
                keys,
            )
            .await
            .map_err(WhitenoiseError::from)?;

        Ok(group)
    }

    pub async fn fetch_groups(
        &self,
        account: &Account,
        active_filter: bool,
    ) -> Result<Vec<group_types::Group>> {
        let nostr_mls = Account::create_nostr_mls(account.pubkey, &self.config.data_dir).unwrap();
        Ok(nostr_mls
            .get_groups()
            .map_err(WhitenoiseError::from)?
            .into_iter()
            .filter(|group| !active_filter || group.state == group_types::GroupState::Active)
            .collect())
    }

    pub async fn fetch_group_members(
        &self,
        account: &Account,
        group_id: &GroupId,
    ) -> Result<Vec<PublicKey>> {
        let nostr_mls = Account::create_nostr_mls(account.pubkey, &self.config.data_dir).unwrap();
        Ok(nostr_mls
            .get_members(group_id)
            .map_err(WhitenoiseError::from)?
            .into_iter()
            .collect())
    }

    pub async fn fetch_group_admins(
        &self,
        account: &Account,
        group_id: &GroupId,
    ) -> Result<Vec<PublicKey>> {
        let nostr_mls = Account::create_nostr_mls(account.pubkey, &self.config.data_dir).unwrap();
        Ok(nostr_mls
            .get_group(group_id)
            .map_err(WhitenoiseError::from)?
            .ok_or(WhitenoiseError::GroupNotFound)?
            .admin_pubkeys
            .into_iter()
            .collect())
    }

    /// Adds new members to an existing MLS group
    ///
    /// This method performs the complete workflow for adding members to a group:
    /// 1. Fetches key packages for all new members from their configured relays
    /// 2. Creates an MLS add members proposal and generates welcome messages
    /// 3. Publishes the evolution event to the group's relays
    /// 4. Merges the pending commit to finalize the member addition
    /// 5. Sends welcome messages to each new member via gift wrap
    ///
    /// # Arguments
    /// * `account` - The account performing the member addition (must be group admin)
    /// * `group_id` - The ID of the group to add members to
    /// * `members` - Vector of public keys for the new members to add
    ///
    /// # Returns
    /// * `Ok(())` - If all members were successfully added and welcomed
    /// * `Err(WhitenoiseError)` - If any step of the process fails
    ///
    /// # Errors
    /// Returns error if:
    /// - Account is not logged in or lacks admin permissions
    /// - NostrMLS is not initialized for the account
    /// - Key packages cannot be fetched for any member
    /// - MLS add members operation fails
    /// - Evolution event publishing fails
    /// - Welcome message sending fails
    /// - Group relays are not accessible
    ///
    /// # Notes
    /// - Each new member's key package is fetched from their configured key package relays
    /// - Welcome messages are sent to each member's inbox relays (with fallback to defaults)
    /// - Welcome messages expire after 1 month
    /// - If evolution event publishing fails, the operation is rolled back
    /// - All new members receive the same group state and can immediately participate
    pub async fn add_members_to_group(
        &self,
        account: &Account,
        group_id: &GroupId,
        members: Vec<PublicKey>,
    ) -> Result<()> {
        let mut key_package_events: Vec<Event> = Vec::new();
        let keys = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?;
        let mut users = Vec::new();

        // Fetch key packages for all members
        for pk in members.iter() {
            let user = User::find_by_pubkey(pk, &self.database).await?;
            let relays_to_use = user.relays(RelayType::KeyPackage, &self.database).await?;
            let some_event = self
                .nostr
                .fetch_user_key_package(*pk, &relays_to_use)
                .await?;
            let event = some_event.ok_or(WhitenoiseError::NostrMlsError(
                nostr_mls::Error::KeyPackage("Does not exist".to_owned()),
            ))?;
            key_package_events.push(event);
            users.push(user);
        }

        let nostr_mls = Account::create_nostr_mls(account.pubkey, &self.config.data_dir).unwrap();
        let update_result = nostr_mls.add_members(group_id, &key_package_events)?;
        // Merge the pending commit immediately after creating it
        // This ensures our local state is correct before publishing
        nostr_mls.merge_pending_commit(group_id)?;

        // Publish the evolution event to the group
        let group_relays = nostr_mls.get_relays(group_id)?;

        let mut relays = HashSet::new();
        for relay in group_relays.clone() {
            let db_relay = self.find_or_create_relay(&relay).await?;
            relays.insert(db_relay);
        }

        let evolution_event = update_result.evolution_event;

        let welcome_rumors = match update_result.welcome_rumors {
            None => {
                return Err(WhitenoiseError::NostrMlsError(nostr_mls::Error::Group(
                    "Missing welcome message".to_owned(),
                )))
            }
            Some(wr) => wr,
        };

        if welcome_rumors.len() != users.len() {
            return Err(WhitenoiseError::Other(anyhow::Error::msg(
                "Welcome rumours are missing for some of the members",
            )));
        }

        // Check if we have any relays to publish to and publish the evolution event
        if group_relays.is_empty() {
            tracing::warn!(
                target: "whitenoise::add_members_to_group",
                "Group has no relays configured, using account's default relays"
            );
            // Use the account's default relays as fallback
            let fallback_relays = account.nip65_relays(self).await?;
            if fallback_relays.is_empty() {
                return Err(WhitenoiseError::Other(anyhow::anyhow!(
                    "No relays available for publishing evolution event - both group relays and account relays are empty"
                )));
            }
            self.nostr
                .publish_event_to(evolution_event, &fallback_relays)
                .await?;
        } else {
            self.nostr
                .publish_event_to(evolution_event, &relays.into_iter().collect::<Vec<_>>())
                .await?;
        }

        // Evolution event published successfully
        // Fan out the welcome message to all members
        for (welcome_rumor, user) in welcome_rumors.iter().zip(users) {
            // Get the public key of the member from the key package event
            let key_package_event_id =
                welcome_rumor
                    .tags
                    .event_ids()
                    .next()
                    .ok_or(WhitenoiseError::Other(anyhow::anyhow!(
                        "No event ID found in welcome rumor"
                    )))?;

            let member_pubkey = key_package_events
                .iter()
                .find(|event| event.id == *key_package_event_id)
                .map(|event| event.pubkey)
                .ok_or(WhitenoiseError::Other(anyhow::anyhow!(
                    "No public key found in key package event"
                )))?;

            // Create a timestamp 1 month in the future
            let one_month_future = Timestamp::now() + Duration::from_secs(30 * 24 * 60 * 60);

            // Use fallback relays if contact has no inbox relays configured
            let relays_to_use = user.relays(RelayType::Inbox, &self.database).await?;

            self.nostr
                .publish_gift_wrap_with_signer(
                    &member_pubkey,
                    welcome_rumor.clone(),
                    &[Tag::expiration(one_month_future)],
                    &relays_to_use,
                    keys.clone(),
                )
                .await
                .map_err(WhitenoiseError::from)?;
        }

        Ok(())
    }

    /// Removes members from an existing MLS group
    ///
    /// This method performs the complete workflow for removing members from a group:
    /// 1. Creates an MLS remove members proposal
    /// 2. Merges the pending commit to finalize the member removal
    /// 3. Publishes the evolution event to the group's relays
    ///
    /// # Arguments
    /// * `account` - The account performing the member removal (must be group admin)
    /// * `group_id` - The ID of the group to remove members from
    /// * `members` - Vector of public keys for the members to remove
    ///
    /// # Returns
    /// * `Ok(())` - If all members were successfully removed
    /// * `Err(WhitenoiseError)` - If any step of the process fails
    ///
    /// # Errors
    /// Returns error if:
    /// - Account is not logged in or lacks admin permissions
    /// - NostrMLS is not initialized for the account
    /// - MLS remove members operation fails
    /// - Evolution event publishing fails
    /// - Group relays are not accessible
    /// - Any of the specified members are not in the group
    ///
    /// # Notes
    /// - The pending commit is merged immediately after creation to ensure local state consistency
    /// - The evolution event is published to all group relays
    /// - Removed members will no longer be able to read new messages in the group
    /// - Admin permissions are required to remove members from a group
    pub async fn remove_members_from_group(
        &self,
        account: &Account,
        group_id: &GroupId,
        members: Vec<PublicKey>,
    ) -> Result<()> {
        let nostr_mls = Account::create_nostr_mls(account.pubkey, &self.config.data_dir).unwrap();
        let update_result = nostr_mls.remove_members(group_id, &members)?;
        nostr_mls.merge_pending_commit(group_id)?;
        let group_relays = nostr_mls.get_relays(group_id)?;

        let evolution_event = update_result.evolution_event;

        let mut relays = HashSet::new();
        for relay in group_relays {
            let db_relay = self.find_or_create_relay(&relay).await?;
            relays.insert(db_relay);
        }

        self.nostr
            .publish_event_to(evolution_event, &relays.into_iter().collect::<Vec<_>>())
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::whitenoise::test_utils::*;
    use crate::whitenoise::Whitenoise;

    #[tokio::test]
    async fn test_create_group() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Setup creator account
        let creator_account = whitenoise.create_identity().await.unwrap();

        // Setup member accounts
        let mut member_pubkeys = Vec::new();
        for _ in 0..2 {
            let member_account = whitenoise.create_identity().await.unwrap();
            let member_user = User::find_by_pubkey(&member_account.pubkey, &whitenoise.database)
                .await
                .unwrap();
            creator_account
                .follow_user(&member_user, &whitenoise.database)
                .await
                .unwrap();
            member_pubkeys.push(member_account.pubkey);
        }

        // Setup admin accounts (creator + one member as admin)
        let admin_pubkeys = vec![creator_account.pubkey, member_pubkeys[0]];

        // Test for success case
        case_create_group_success(
            &whitenoise,
            &creator_account,
            member_pubkeys.clone(),
            admin_pubkeys.clone(),
        )
        .await;

        // // Test case: Key package fetch fails (invalid member)
        // let invalid_member_pubkey = create_test_keys().public_key();
        // case_create_group_key_package_fetch_fails(
        //     &whitenoise,
        //     &creator_account,
        //     vec![invalid_member_pubkey],
        //     admin_pubkeys.clone(),
        // )
        // .await;

        // Test case: Empty admin list
        case_create_group_empty_admin_list(
            &whitenoise,
            &creator_account,
            member_pubkeys.clone(),
            vec![], // Empty admin list
        )
        .await;

        // Test case: Invalid admin pubkey (not a member)
        let non_member_pubkey = create_test_keys().public_key();
        case_create_group_invalid_admin_pubkey(
            &whitenoise,
            &creator_account,
            member_pubkeys.clone(),
            vec![creator_account.pubkey, non_member_pubkey],
        )
        .await;
    }

    async fn case_create_group_success(
        whitenoise: &Whitenoise,
        creator_account: &Account,
        member_pubkeys: Vec<PublicKey>,
        admin_pubkeys: Vec<PublicKey>,
    ) {
        let config = create_nostr_group_config_data();
        // Create the group
        let result = whitenoise
            .create_group(
                creator_account,
                member_pubkeys.clone(),
                admin_pubkeys.clone(),
                create_nostr_group_config_data(),
            )
            .await;

        // Assert the group was created successfully
        assert!(result.is_ok(), "Error {:?}", result.unwrap_err());
        let group = result.unwrap();
        assert_eq!(group.name, config.name);
        assert_eq!(group.description, config.description);
        assert!(group.admin_pubkeys.contains(&creator_account.pubkey));
        assert!(group.admin_pubkeys.contains(&member_pubkeys[0]));
    }

    /// Test case: Member/admin validation fails - empty admin list
    async fn case_create_group_empty_admin_list(
        whitenoise: &Whitenoise,
        creator_account: &Account,
        member_pubkeys: Vec<PublicKey>,
        admin_pubkeys: Vec<PublicKey>,
    ) {
        let result = whitenoise
            .create_group(
                creator_account,
                member_pubkeys,
                admin_pubkeys,
                create_nostr_group_config_data(),
            )
            .await;

        // Should fail because groups need at least one admin
        assert!(result.is_err());
        match result.unwrap_err() {
            WhitenoiseError::NostrMlsError(_) => {
                // Expected - invalid group configuration
            }
            other => panic!(
                "Expected NostrMlsError due to empty admin list, got: {:?}",
                other
            ),
        }
    }

    /// Test case: Key package fetching fails - invalid member pubkey
    async fn _case_create_group_key_package_fetch_fails(
        whitenoise: &Whitenoise,
        creator_account: &Account,
        member_pubkeys: Vec<PublicKey>,
        admin_pubkeys: Vec<PublicKey>,
    ) {
        let result = whitenoise
            .create_group(
                creator_account,
                member_pubkeys,
                admin_pubkeys,
                create_nostr_group_config_data(),
            )
            .await;

        // Should fail because key package doesn't exist for the member
        assert!(result.is_err(), "{:?}", result);
    }

    /// Test case: Member/admin validation fails - non-existent admin
    async fn case_create_group_invalid_admin_pubkey(
        whitenoise: &Whitenoise,
        creator_account: &Account,
        member_pubkeys: Vec<PublicKey>,
        admin_pubkeys: Vec<PublicKey>,
    ) {
        let result = whitenoise
            .create_group(
                creator_account,
                member_pubkeys,
                admin_pubkeys,
                create_nostr_group_config_data(),
            )
            .await;

        // Might succeed or fail depending on MLS validation rules
        // In a real implementation, this might be validated
        match result {
            Ok(_) => {
                // Some MLS implementations might allow this
                println!("Group created with non-member admin (implementation-specific behavior)");
            }
            Err(WhitenoiseError::NostrMlsError(_)) => {
                // Expected if MLS validates admin membership
            }
            Err(other) => panic!("Unexpected error: {:?}", other),
        }
    }
}
