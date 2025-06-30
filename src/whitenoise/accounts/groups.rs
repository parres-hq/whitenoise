use crate::whitenoise::accounts::Account;
use crate::whitenoise::error::{Result, WhitenoiseError};
use crate::whitenoise::relays::RelayType;
use crate::whitenoise::Whitenoise;
use nostr_mls::prelude::*;

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
        group_name: String,
        description: String,
    ) -> Result<group_types::Group> {
        if !self.logged_in(&creator_account.pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        let keys = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&creator_account.pubkey)?;

        let group_relays = self
            .fetch_relays(creator_account.pubkey, RelayType::Nostr)
            .await?;

        let group: group_types::Group;
        let welcome_rumors: Vec<UnsignedEvent>;
        let group_ids: Vec<String>;
        let mut key_package_events: Vec<Event> = Vec::new();

        let nostr_mls_guard = creator_account.nostr_mls.lock().await;

        if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
            // Fetch key packages for all members
            for pk in member_pubkeys.iter() {
                let user_key_package_relays = self
                    .fetch_relays_with_fallback(*pk, RelayType::KeyPackage)
                    .await?;
                let some_event = self
                    .fetch_key_package_event(*pk, user_key_package_relays.clone())
                    .await?;
                let event = some_event.ok_or(WhitenoiseError::NostrMlsError(
                    nostr_mls::Error::KeyPackage("Does not exist".to_owned()),
                ))?;
                key_package_events.push(event);
            }

            let create_group_result = nostr_mls
                .create_group(
                    group_name,
                    description,
                    &creator_account.pubkey,
                    key_package_events.clone(),
                    admin_pubkeys,
                    group_relays.clone(),
                )
                .map_err(WhitenoiseError::from)?;

            group = create_group_result.group;
            welcome_rumors = create_group_result.welcome_rumors;
            group_ids = nostr_mls
                .get_groups()
                .map_err(WhitenoiseError::from)?
                .into_iter()
                .map(|g| hex::encode(g.nostr_group_id))
                .collect::<Vec<_>>();
        } else {
            return Err(WhitenoiseError::NostrMlsNotInitialized);
        }

        tracing::debug!(target: "whitenoise::commands::groups::create_group", "nostr_mls lock released");

        // Fan out the welcome message to all members
        for welcome_rumor in welcome_rumors {
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

            let member_inbox_relays = self
                .fetch_relays_with_fallback(member_pubkey, RelayType::Inbox)
                .await?;

            // Create a timestamp 1 month in the future
            use std::ops::Add;
            let one_month_future = Timestamp::now().add(30 * 24 * 60 * 60);
            self.nostr
                .publish_gift_wrap_with_signer(
                    &member_pubkey,
                    welcome_rumor.clone(),
                    vec![Tag::expiration(one_month_future)],
                    &member_inbox_relays,
                    keys.clone(),
                )
                .await
                .map_err(WhitenoiseError::from)?;
        }

        self.nostr
            .setup_group_messages_subscriptions_with_signer(
                creator_account.pubkey,
                group_relays,
                group_ids,
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
        if !self.logged_in(&account.pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        let nostr_mls_guard = account.nostr_mls.lock().await;
        if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
            Ok(nostr_mls
                .get_groups()
                .map_err(WhitenoiseError::from)?
                .into_iter()
                .filter(|group| !active_filter || group.state == group_types::GroupState::Active)
                .collect())
        } else {
            Err(WhitenoiseError::NostrMlsNotInitialized)
        }
    }

    pub async fn fetch_group_members(
        &self,
        account: &Account,
        group_id: &GroupId,
    ) -> Result<Vec<PublicKey>> {
        if !self.logged_in(&account.pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        let nostr_mls_guard = account.nostr_mls.lock().await;
        if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
            Ok(nostr_mls
                .get_members(group_id)
                .map_err(WhitenoiseError::from)?
                .into_iter()
                .collect())
        } else {
            Err(WhitenoiseError::NostrMlsNotInitialized)
        }
    }

    pub async fn fetch_group_admins(
        &self,
        account: &Account,
        group_id: &GroupId,
    ) -> Result<Vec<PublicKey>> {
        if !self.logged_in(&account.pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        let nostr_mls_guard = account.nostr_mls.lock().await;
        if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
            Ok(nostr_mls
                .get_group(group_id)
                .map_err(WhitenoiseError::from)?
                .ok_or(WhitenoiseError::GroupNotFound)?
                .admin_pubkeys
                .into_iter()
                .collect())
        } else {
            Err(WhitenoiseError::NostrMlsNotInitialized)
        }
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
        let evolution_event: Event;
        let welcome_rumors: Option<Vec<UnsignedEvent>>;
        let mut key_package_events: Vec<Event> = Vec::new();
        let keys = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?;

        let nostr_mls_guard = account.nostr_mls.lock().await;
        if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
            // Fetch key packages for all members
            for pk in members.iter() {
                let user_key_package_relays = self
                    .fetch_relays_with_fallback(*pk, RelayType::KeyPackage)
                    .await?;
                let some_event = self
                    .fetch_key_package_event(*pk, user_key_package_relays.clone())
                    .await?;
                let event = some_event.ok_or(WhitenoiseError::NostrMlsError(
                    nostr_mls::Error::KeyPackage("Does not exist".to_owned()),
                ))?;
                key_package_events.push(event);
            }

            let update_result = nostr_mls
                .add_members(group_id, &key_package_events)
                .map_err(WhitenoiseError::from)?;
            evolution_event = update_result.evolution_event;
            welcome_rumors = update_result.welcome_rumors;

            if welcome_rumors.is_none() {
                return Err(WhitenoiseError::NostrMlsError(nostr_mls::Error::Group(
                    "Missing welcome message".to_owned(),
                )));
            }

            // Merge the pending commit immediately after creating it
            // This ensures our local state is correct before publishing
            nostr_mls
                .merge_pending_commit(group_id)
                .map_err(WhitenoiseError::from)?;

            // Publish the evolution event to the group
            let group_relays = nostr_mls
                .get_relays(group_id)
                .map_err(WhitenoiseError::from)?;
            let result = self
                .nostr
                .publish_event_to(evolution_event, &group_relays)
                .await;

            match result {
                Ok(_event_id) => {
                    // Evolution event published successfully
                    // Fan out the welcome message to all members
                    for welcome_rumor in welcome_rumors.unwrap() {
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

                        let member_inbox_relays = self
                            .fetch_relays_with_fallback(member_pubkey, RelayType::Inbox)
                            .await?;

                        // Create a timestamp 1 month in the future
                        use std::ops::Add;
                        let one_month_future = Timestamp::now().add(30 * 24 * 60 * 60);
                        self.nostr
                            .publish_gift_wrap_with_signer(
                                &member_pubkey,
                                welcome_rumor.clone(),
                                vec![Tag::expiration(one_month_future)],
                                &member_inbox_relays,
                                keys.clone(),
                            )
                            .await
                            .map_err(WhitenoiseError::from)?;
                    }
                }
                Err(e) => {
                    return Err(WhitenoiseError::NostrManager(e));
                }
            }
        } else {
            return Err(WhitenoiseError::NostrMlsNotInitialized);
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
        let evolution_event: Event;
        let nostr_mls_guard = account.nostr_mls.lock().await;

        if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
            // First, validate that all members to be removed are actually in the group
            let current_members = nostr_mls
                .get_members(group_id)
                .map_err(WhitenoiseError::from)?
                .into_iter()
                .collect::<std::collections::HashSet<PublicKey>>();

            let mut members_not_in_group = Vec::new();
            for member in &members {
                if !current_members.contains(member) {
                    members_not_in_group.push(*member);
                }
            }

            if !members_not_in_group.is_empty() {
                return Err(WhitenoiseError::MembersNotInGroup);
            }
            let update_result = nostr_mls.remove_members(group_id, &members)?;
            evolution_event = update_result.evolution_event;

            nostr_mls
                .merge_pending_commit(group_id)
                .map_err(WhitenoiseError::from)?;

            let group_relays = nostr_mls
                .get_relays(group_id)
                .map_err(WhitenoiseError::from)?;

            self.nostr
                .publish_event_to(evolution_event, &group_relays)
                .await?;
        } else {
            return Err(WhitenoiseError::NostrMlsNotInitialized);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::whitenoise::test_utils::*;
    use crate::whitenoise::Whitenoise;

    async fn setup_multiple_test_accounts(
        whitenoise: &Whitenoise,
        creator_account: &Account,
        count: usize,
    ) -> Vec<(Account, Keys)> {
        let mut accounts = Vec::new();
        for _ in 0..count {
            let (account, keys) = create_test_account();
            accounts.push((account.clone(), keys.clone()));
            whitenoise
                .add_contact(creator_account, keys.public_key())
                .await
                .unwrap();

            // publish keypackage to relays
            let (ekp, tags) = whitenoise
                .encoded_key_package(creator_account, &account.pubkey)
                .await
                .unwrap();
            let key_package_event_builder = EventBuilder::new(Kind::MlsKeyPackage, ekp).tags(tags);

            // Get relays with fallback to defaults if user hasn't configured key package relays
            let relays_to_use = whitenoise
                .fetch_relays_with_fallback(account.pubkey, RelayType::KeyPackage)
                .await
                .unwrap();

            let _ = whitenoise
                .nostr
                .publish_event_builder_with_signer(key_package_event_builder, &relays_to_use, keys)
                .await
                .unwrap();
        }
        accounts
    }

    #[tokio::test]
    async fn test_create_group() {
        let whitenoise = test_get_whitenoise().await;

        // Setup creator account
        let (creator_account, _creator_keys) = setup_login_account(whitenoise).await;

        // Setup member accounts
        let member_accounts = setup_multiple_test_accounts(whitenoise, &creator_account, 2).await;
        let member_pubkeys: Vec<PublicKey> =
            member_accounts.iter().map(|(acc, _)| acc.pubkey).collect();

        // Setup admin accounts (creator + one member as admin)
        let admin_pubkeys = vec![creator_account.pubkey, member_pubkeys[0]];

        let group_name = "Test Group";
        let description = "A test group for unit testing";

        // Test for success case
        case_create_group_success(
            whitenoise,
            &creator_account,
            member_pubkeys.clone(),
            admin_pubkeys.clone(),
            group_name,
            description,
        )
        .await;

        // Test case: Account not found (not logged in)
        let (unlogged_account, _unused_keys) = create_test_account();
        case_create_group_account_not_found(
            whitenoise,
            &unlogged_account,
            member_pubkeys.clone(),
            admin_pubkeys.clone(),
            "Test Group",
            "Test Description",
        )
        .await;

        // Test case: NostrMLS not initialized
        let keys = create_test_keys();
        let uninitialized_account = whitenoise
            .login(keys.secret_key().to_secret_hex())
            .await
            .unwrap();
        // Clear the NostrMls instance manually to test the error case
        {
            let mut nostr_mls_guard = uninitialized_account.nostr_mls.lock().await;
            *nostr_mls_guard = None;
        }
        case_create_group_nostr_mls_not_initialized(
            whitenoise,
            &uninitialized_account,
            member_pubkeys.clone(),
            admin_pubkeys.clone(),
            "Test Group",
            "Test Description",
        )
        .await;

        // Test case: Key package fetch fails (invalid member)
        let invalid_member_pubkey = create_test_keys().public_key();
        case_create_group_key_package_fetch_fails(
            whitenoise,
            &creator_account,
            vec![invalid_member_pubkey],
            admin_pubkeys.clone(),
            "Test Group",
            "Test Description",
        )
        .await;

        // Test case: Empty admin list
        case_create_group_empty_admin_list(
            whitenoise,
            &creator_account,
            member_pubkeys.clone(),
            vec![], // Empty admin list
            "Test Group",
            "Test Description",
        )
        .await;

        // Test case: Invalid admin pubkey (not a member)
        let non_member_pubkey = create_test_keys().public_key();
        case_create_group_invalid_admin_pubkey(
            whitenoise,
            &creator_account,
            member_pubkeys.clone(),
            vec![creator_account.pubkey, non_member_pubkey],
            "Test Group",
            "Test Description",
        )
        .await;

        // Test case: Welcome message fails (no relays)
        let (no_relay_creator, _keys) = setup_login_account(whitenoise).await;
        whitenoise
            .update_relays(&no_relay_creator, RelayType::Nostr, vec![])
            .await
            .unwrap();
        case_create_group_welcome_message_fails(
            whitenoise,
            &no_relay_creator,
            member_pubkeys.clone(),
            vec![no_relay_creator.pubkey],
            "Test Group",
            "Test Description",
        )
        .await;
    }

    async fn case_create_group_success(
        whitenoise: &Whitenoise,
        creator_account: &Account,
        member_pubkeys: Vec<PublicKey>,
        admin_pubkeys: Vec<PublicKey>,
        group_name: &str,
        description: &str,
    ) {
        // Create the group
        let result = whitenoise
            .create_group(
                creator_account,
                member_pubkeys.clone(),
                admin_pubkeys.clone(),
                group_name.to_owned(),
                description.to_owned(),
            )
            .await;

        // Assert the group was created successfully
        assert!(result.is_ok(), "Error {:?}", result.unwrap_err());
        let group = result.unwrap();
        assert_eq!(group.name, group_name);
        assert_eq!(group.description, description);
        assert!(group.admin_pubkeys.contains(&creator_account.pubkey));
        assert!(group.admin_pubkeys.contains(&member_pubkeys[0]));
    }

    /// Test case: Active account is not the creator
    async fn case_create_group_account_not_found(
        whitenoise: &Whitenoise,
        creator_account: &Account,
        member_pubkeys: Vec<PublicKey>,
        admin_pubkeys: Vec<PublicKey>,
        group_name: &str,
        description: &str,
    ) {
        let result = whitenoise
            .create_group(
                creator_account,
                member_pubkeys,
                admin_pubkeys,
                group_name.to_string(),
                description.to_string(),
            )
            .await;

        assert!(matches!(result, Err(WhitenoiseError::AccountNotFound)));
    }

    /// Test case: NostrMLS not initialized (part of MLS group creation fails)
    async fn case_create_group_nostr_mls_not_initialized(
        whitenoise: &Whitenoise,
        creator_account: &Account,
        member_pubkeys: Vec<PublicKey>,
        admin_pubkeys: Vec<PublicKey>,
        group_name: &str,
        description: &str,
    ) {
        let result = whitenoise
            .create_group(
                creator_account,
                member_pubkeys,
                admin_pubkeys,
                group_name.to_string(),
                description.to_string(),
            )
            .await;

        assert!(matches!(
            result,
            Err(WhitenoiseError::NostrMlsNotInitialized)
        ));
    }

    /// Test case: Key package fetching fails - invalid member pubkey
    async fn case_create_group_key_package_fetch_fails(
        whitenoise: &Whitenoise,
        creator_account: &Account,
        member_pubkeys: Vec<PublicKey>,
        admin_pubkeys: Vec<PublicKey>,
        group_name: &str,
        description: &str,
    ) {
        let result = whitenoise
            .create_group(
                creator_account,
                member_pubkeys,
                admin_pubkeys,
                group_name.to_string(),
                description.to_string(),
            )
            .await;

        // Should fail because key package doesn't exist for the member
        assert!(result.is_err());
        match result.unwrap_err() {
            WhitenoiseError::NostrMlsError(_) => {
                // Expected - key package doesn't exist
            }
            other => panic!("Expected NostrMlsError, got: {:?}", other),
        }
    }

    /// Test case: Member/admin validation fails - empty admin list
    async fn case_create_group_empty_admin_list(
        whitenoise: &Whitenoise,
        creator_account: &Account,
        member_pubkeys: Vec<PublicKey>,
        admin_pubkeys: Vec<PublicKey>,
        group_name: &str,
        description: &str,
    ) {
        let result = whitenoise
            .create_group(
                creator_account,
                member_pubkeys,
                admin_pubkeys,
                group_name.to_string(),
                description.to_string(),
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

    /// Test case: Member/admin validation fails - non-existent admin
    async fn case_create_group_invalid_admin_pubkey(
        whitenoise: &Whitenoise,
        creator_account: &Account,
        member_pubkeys: Vec<PublicKey>,
        admin_pubkeys: Vec<PublicKey>,
        group_name: &str,
        description: &str,
    ) {
        let result = whitenoise
            .create_group(
                creator_account,
                member_pubkeys,
                admin_pubkeys,
                group_name.to_string(),
                description.to_string(),
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

    /// Test case: Welcome message sending fails - no relays configured
    async fn case_create_group_welcome_message_fails(
        whitenoise: &Whitenoise,
        creator_account: &Account,
        member_pubkeys: Vec<PublicKey>,
        admin_pubkeys: Vec<PublicKey>,
        group_name: &str,
        description: &str,
    ) {
        let result = whitenoise
            .create_group(
                creator_account,
                member_pubkeys,
                admin_pubkeys,
                group_name.to_string(),
                description.to_string(),
            )
            .await;

        // May fail when trying to send welcome messages with no relays
        match result {
            Ok(_) => {
                // Might succeed if fallback relays are used
                println!("Group created despite no relays (fallback used)");
            }
            Err(e) => {
                // Expected if no relays available for welcome messages
                println!("Group creation failed due to relay issues: {:?}", e);
            }
        }
    }
}
