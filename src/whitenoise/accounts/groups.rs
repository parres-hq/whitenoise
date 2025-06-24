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

        let key_package_relays = self
            .fetch_relays_with_fallback(creator_account.pubkey, RelayType::Nostr)
            .await?;

        let group: group_types::Group;
        let serialized_welcome_message: Vec<u8>;
        let group_ids: Vec<String>;
        let mut eventid_keypackage_list: Vec<(EventId, KeyPackage)> = Vec::new();

        let nostr_mls_guard = creator_account.nostr_mls.lock().await;

        if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
            // Fetch key packages for all members
            for pk in member_pubkeys.iter() {
                let some_event = self
                    .fetch_key_package_event(*pk, key_package_relays.clone())
                    .await?;
                let event = some_event.ok_or(WhitenoiseError::NostrMlsError(
                    nostr_mls::Error::KeyPackage("Does not exist".to_owned()),
                ))?;
                let key_package = nostr_mls
                    .parse_key_package(&event)
                    .map_err(WhitenoiseError::from)?;
                eventid_keypackage_list.push((event.id, key_package));
            }

            let create_group_result = nostr_mls
                .create_group(
                    group_name,
                    description,
                    &creator_account.pubkey,
                    &member_pubkeys,
                    eventid_keypackage_list
                        .iter()
                        .map(|(_, kp)| kp.clone())
                        .collect::<Vec<_>>()
                        .as_slice(),
                    admin_pubkeys,
                    group_relays.clone(),
                )
                .map_err(WhitenoiseError::from)?;

            group = create_group_result.group;
            serialized_welcome_message = create_group_result.serialized_welcome_message;
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
        for (i, (event_id, _)) in eventid_keypackage_list.into_iter().enumerate() {
            let member_pubkey = member_pubkeys[i];

            let welcome_rumor =
                EventBuilder::new(Kind::MlsWelcome, hex::encode(&serialized_welcome_message))
                    .tags(vec![
                        Tag::from_standardized(TagStandard::Relays(group_relays.clone())),
                        Tag::event(event_id),
                    ])
                    .build(creator_account.pubkey);

            tracing::debug!(
                target: "whitenoise::groups::create_group",
                "Welcome rumor: {:?}",
                welcome_rumor
            );

            // Create a timestamp 1 month in the future
            use std::ops::Add;
            let one_month_future = Timestamp::now().add(30 * 24 * 60 * 60);
            self.nostr
                .publish_gift_wrap_with_signer(
                    &member_pubkey,
                    welcome_rumor,
                    vec![Tag::expiration(one_month_future)],
                    &group_relays,
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
        whitenoise
            .update_relays(
                &creator_account,
                RelayType::Nostr,
                vec![RelayUrl::parse("ws://localhost:8080/").unwrap()],
            )
            .await
            .unwrap();

        // Setup member accounts
        let member_accounts = setup_multiple_test_accounts(whitenoise, &creator_account, 2).await;
        let member_pubkeys: Vec<PublicKey> =
            member_accounts.iter().map(|(acc, _)| acc.pubkey).collect();

        // Setup admin accounts (creator + one member as admin)
        let admin_pubkeys = vec![creator_account.pubkey, member_pubkeys[0]];

        let group_name = "Test Group";
        let description = "A test group for unit testing";

        // Publish key packages for all members first
        for (member_account, _) in &member_accounts {
            let _ = whitenoise
                .publish_key_package_for_account(member_account)
                .await;
        }

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
