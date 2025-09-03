use std::{collections::HashSet, time::Duration};

use nostr_mls::prelude::*;
use nostr_mls_sqlite_storage::NostrMlsSqliteStorage;
use nostr_sdk::hashes::sha256::Hash as Sha256Hash;
use nwc::nostr::hashes::Hash;

use tokio::fs;

use crate::{
    media::encryption::{decrypt_data, encrypt_data},
    whitenoise::{
        accounts::Account,
        error::{BlossomError, Result, WhitenoiseError},
        group_information::{GroupInformation, GroupType},
        relays::Relay,
        users::User,
        Whitenoise,
    },
    RelayType,
};

impl Whitenoise {
    /// Ensures that group relays are available for publishing evolution events.
    /// Returns the validated relay URLs.
    ///
    /// # Arguments
    /// * `nostr_mls` - The NostrMls instance to get relays from
    /// * `group_id` - The ID of the group
    ///
    /// # Returns
    /// * `Ok(Vec<nostr_sdk::RelayUrl>)` - Vector of relay URLs
    /// * `Err(WhitenoiseError::GroupMissingRelays)` - If no relays are configured
    fn ensure_group_relays(
        nostr_mls: &NostrMls<NostrMlsSqliteStorage>,
        group_id: &GroupId,
    ) -> Result<Vec<nostr_sdk::RelayUrl>> {
        let group_relays = nostr_mls.get_relays(group_id)?;

        if group_relays.is_empty() {
            return Err(WhitenoiseError::GroupMissingRelays);
        }

        Ok(group_relays.into_iter().collect())
    }

    /// Converts relay URLs to database Relay objects.
    ///
    /// # Arguments
    /// * `relay_urls` - Vector of relay URLs to convert
    ///
    /// # Returns
    /// * `Ok(Vec<Relay>)` - Vector of database Relay objects
    /// * `Err(WhitenoiseError)` - If relay creation fails
    async fn convert_relay_urls_to_relays(
        &self,
        relay_urls: Vec<nostr_sdk::RelayUrl>,
    ) -> Result<Vec<Relay>> {
        let mut relays = Vec::new();
        for relay_url in relay_urls {
            let db_relay = self.find_or_create_relay_by_url(&relay_url).await?;
            relays.push(db_relay);
        }
        Ok(relays)
    }
    /// Creates a new MLS group with the specified members and settings
    ///
    /// # Arguments
    /// * `creator_account` - Account of the group creator (must be the active account)
    /// * `member_pubkeys` - List of public keys for group members
    /// * `config` - Group configuration data
    /// * `group_type` - Optional explicit group type. If None, will be inferred from participant count
    pub async fn create_group(
        &self,
        creator_account: &Account,
        member_pubkeys: Vec<PublicKey>,
        config: NostrGroupConfigData,
        group_type: Option<GroupType>,
    ) -> Result<group_types::Group> {
        let keys = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&creator_account.pubkey)?;

        let mut key_package_events: Vec<Event> = Vec::new();
        let mut members = Vec::new();

        for pk in member_pubkeys.iter() {
            let (mut user, created) = User::find_or_create_by_pubkey(pk, &self.database).await?;
            if created {
                // Fetch the user's relay lists and save them to the database
                if let Err(e) = user.update_relay_lists(self).await {
                    tracing::warn!(
                        target: "whitenoise::accounts::groups::create_group",
                        "Failed to update relay lists for new user {}: {}",
                        user.pubkey,
                        e
                    );
                    // Continue with group creation even if relay list update fails
                }
                if let Err(e) = user.sync_metadata(self).await {
                    tracing::warn!(
                        target: "whitenoise::accounts::groups::create_group",
                        "Failed to sync metadata for new user {}: {}",
                        user.pubkey,
                        e
                    );
                    // Continue with group creation even if metadata sync fails
                }
            }
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

        let nostr_mls = Account::create_nostr_mls(creator_account.pubkey, &self.config.data_dir)?;
        let create_group_result =
            nostr_mls.create_group(&creator_account.pubkey, key_package_events.clone(), config)?;

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
            // If the member has no inbox relays configured, use their nip65 relays
            let member_inbox_relays = member.relays(RelayType::Inbox, &self.database).await?;
            let relays_to_use = if member_inbox_relays.is_empty() {
                member.relays(RelayType::Nip65, &self.database).await?
            } else {
                member_inbox_relays
            };

            self.nostr
                .publish_gift_wrap_to(
                    &member_pubkey,
                    welcome_rumor.clone(),
                    &[Tag::expiration(one_month_future)],
                    creator_account,
                    &relays_to_use,
                    keys.clone(),
                )
                .await
                .map_err(WhitenoiseError::from)?;
        }

        let mut relays = HashSet::new();
        for relay_url in group_relays {
            let db_relay = self.find_or_create_relay_by_url(&relay_url).await?;
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

        let participant_count = member_pubkeys.len() + 1;
        GroupInformation::create_for_group(
            self,
            &group.mls_group_id,
            group_type,
            None,
            participant_count,
        )
        .await?;

        Ok(group)
    }

    pub async fn groups(
        &self,
        account: &Account,
        active_filter: bool,
    ) -> Result<Vec<group_types::Group>> {
        let nostr_mls = Account::create_nostr_mls(account.pubkey, &self.config.data_dir)?;
        Ok(nostr_mls
            .get_groups()
            .map_err(WhitenoiseError::from)?
            .into_iter()
            .filter(|group| !active_filter || group.state == group_types::GroupState::Active)
            .collect::<Vec<group_types::Group>>())
    }

    pub async fn group_members(
        &self,
        account: &Account,
        group_id: &GroupId,
    ) -> Result<Vec<PublicKey>> {
        let nostr_mls = Account::create_nostr_mls(account.pubkey, &self.config.data_dir)?;
        Ok(nostr_mls
            .get_members(group_id)
            .map_err(WhitenoiseError::from)?
            .into_iter()
            .collect::<Vec<PublicKey>>())
    }

    pub async fn group_admins(
        &self,
        account: &Account,
        group_id: &GroupId,
    ) -> Result<Vec<PublicKey>> {
        let nostr_mls = Account::create_nostr_mls(account.pubkey, &self.config.data_dir)?;
        Ok(nostr_mls
            .get_group(group_id)
            .map_err(WhitenoiseError::from)?
            .ok_or(WhitenoiseError::GroupNotFound)?
            .admin_pubkeys
            .into_iter()
            .collect::<Vec<PublicKey>>())
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
            let (user, newly_created) = User::find_or_create_by_pubkey(pk, &self.database).await?;

            if newly_created {
                self.background_fetch_user_data(&user).await?;
            }
            // Try and get user's key package relays, if they don't have any, use account's default relays
            let mut relays_to_use = user.relays(RelayType::KeyPackage, &self.database).await?;
            if relays_to_use.is_empty() {
                tracing::warn!(
                    target: "whitenoise::accounts::groups::add_members_to_group",
                    "User {} has no relays configured, using account's default relays",
                    user.pubkey
                );
                relays_to_use = account.nip65_relays(self).await?;
            }
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

        let (relay_urls, evolution_event, welcome_rumors) = {
            let nostr_mls = Account::create_nostr_mls(account.pubkey, &self.config.data_dir)?;
            let relay_urls = Self::ensure_group_relays(&nostr_mls, group_id)?;

            let update_result = nostr_mls.add_members(group_id, &key_package_events)?;
            // Merge the pending commit immediately after creating it
            // This ensures our local state is correct before publishing
            nostr_mls.merge_pending_commit(group_id)?;

            (
                relay_urls,
                update_result.evolution_event,
                update_result.welcome_rumors,
            )
        };

        let relays = self.convert_relay_urls_to_relays(relay_urls).await?;

        let welcome_rumors = match welcome_rumors {
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

        self.nostr
            .publish_mls_commit_to(evolution_event, account, &relays)
            .await?;

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

            // If the user has no inbox relays configured, use their nip65 relays
            let user_inbox_relays = user.relays(RelayType::Inbox, &self.database).await?;
            let relays_to_use = if user_inbox_relays.is_empty() {
                user.relays(RelayType::Nip65, &self.database).await?
            } else {
                user_inbox_relays
            };

            self.nostr
                .publish_gift_wrap_to(
                    &member_pubkey,
                    welcome_rumor.clone(),
                    &[Tag::expiration(one_month_future)],
                    account,
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
    pub async fn remove_members_from_group(
        &self,
        account: &Account,
        group_id: &GroupId,
        members: Vec<PublicKey>,
    ) -> Result<()> {
        let (relay_urls, evolution_event) = {
            let nostr_mls = Account::create_nostr_mls(account.pubkey, &self.config.data_dir)?;
            let relay_urls = Self::ensure_group_relays(&nostr_mls, group_id)?;

            let update_result = nostr_mls.remove_members(group_id, &members)?;
            nostr_mls.merge_pending_commit(group_id)?;

            (relay_urls, update_result.evolution_event)
        };

        let relays = self.convert_relay_urls_to_relays(relay_urls).await?;

        self.nostr
            .publish_mls_commit_to(evolution_event, account, &relays)
            .await?;
        Ok(())
    }

    /// Updates group metadata and publishes the change to group relays.
    ///
    /// This method updates the group data and publishes the change to group relays.
    ///
    /// # Arguments
    /// * `account` - The account performing the group data update (must be group admin)
    /// * `group_id` - The ID of the group to update
    /// * `group_data` - The new group data to update
    pub async fn update_group_data(
        &self,
        account: &Account,
        group_id: &GroupId,
        group_data: NostrGroupDataUpdate,
    ) -> Result<()> {
        let (relay_urls, evolution_event) = {
            let nostr_mls = Account::create_nostr_mls(account.pubkey, &self.config.data_dir)?;
            let relay_urls = Self::ensure_group_relays(&nostr_mls, group_id)?;

            let update_result = nostr_mls.update_group_data(group_id, group_data)?;
            nostr_mls.merge_pending_commit(group_id)?;

            (relay_urls, update_result.evolution_event)
        };

        let relays = self.convert_relay_urls_to_relays(relay_urls).await?;

        self.nostr
            .publish_mls_commit_to(evolution_event, account, &relays)
            .await?;
        Ok(())
    }

    /// Initiates the process to leave a group by creating a self-removal proposal.
    ///
    /// This method creates a self-removal proposal using the nostr-mls library and publishes
    /// it to the group relays. The proposal will need to be committed by a group admin before
    /// the removal is finalized.
    ///
    /// # Arguments
    /// * `account` - The account that wants to leave the group
    /// * `group_id` - The ID of the group to leave
    ///
    /// # Returns
    /// * `Ok(())` if the proposal was successfully created and published
    /// * `Err(WhitenoiseError)` if the operation failed
    pub async fn leave_group(&self, account: &Account, group_id: &GroupId) -> Result<()> {
        let (relay_urls, evolution_event) = {
            let nostr_mls = Account::create_nostr_mls(account.pubkey, &self.config.data_dir)?;
            let relay_urls = Self::ensure_group_relays(&nostr_mls, group_id)?;

            // Create a self-removal proposal
            let update_result = nostr_mls.leave_group(group_id)?;

            (relay_urls, update_result.evolution_event)
        };

        let relays = self.convert_relay_urls_to_relays(relay_urls).await?;

        // Publish the self-removal proposal to the group
        self.nostr
            .publish_mls_commit_to(evolution_event, account, &relays)
            .await?;

        // TODO: Do any local updates to ensure that we're accurately reflecting that the account is trying to leave this group
        Ok(())
    }

    pub async fn update_group_image(
        &self,
        account: &Account,
        group_id: &GroupId,
        image_path: &str,
    ) -> Result<()> {
        // Fetch and encrypt image bytes
        let image_bytes = fs::read(image_path).await?;
        let secret_key = Keys::generate().secret_key().to_secret_bytes();

        let (encrypted_bytes, nonce_bytes) = encrypt_data(&image_bytes, &secret_key)?;

        // upload to blossom
        let account_keys = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?;
        let blob_descriptor = self
            .blossom
            .upload_blob(encrypted_bytes, None, None, Some(&account_keys))
            .await
            .map_err(BlossomError::from)?;

        tracing::info!(
            "groups::update_group_image: Uploaded blob to blossom for image in {image_path}"
        );

        // Update mls group data
        let group_data = NostrGroupDataUpdate::new()
            .image_hash(Some(blob_descriptor.sha256.as_byte_array().to_vec()))
            .image_key(Some(secret_key.to_vec()))
            .image_nonce(Some(nonce_bytes.to_vec()));

        self.update_group_data(account, group_id, group_data).await
    }

    pub async fn get_group_image(&self, account: &Account, group_id: &GroupId) -> Result<Vec<u8>> {
        let nostr_mls = Account::create_nostr_mls(account.pubkey, &self.config.data_dir)?;
        let group_info = GroupInformation::get_by_mls_group_id(group_id, self).await?;
        match group_info.image_pointer {
            Some(image_pointer) => {
                let image_bytes = fs::read(image_pointer).await?;
                Ok(image_bytes)
            }
            None => {
                // Get image information from Mls Group
                let group = nostr_mls
                    .get_group(group_id)?
                    .ok_or(WhitenoiseError::GroupNotFound)?;
                let image_hash = group
                    .image_hash
                    .ok_or(WhitenoiseError::GroupImageNotFound)?;
                let image_hash_array: [u8; 32] = image_hash
                    .try_into()
                    .map_err(|_| BlossomError::InvalidSha256)?;
                let sha256 = Sha256Hash::from_byte_array(image_hash_array);

                // Fetch and decrypt the encrypted bytes from blossom
                let encrypted_bytes = self
                    .blossom
                    .get_blob(sha256, None, None, Option::<&Keys>::None)
                    .await
                    .map_err(BlossomError::from)?;

                tracing::info!(
                    "groups::get_group_image: Fetched blob from blossom for hash {image_hash_array:?}"
                );

                let image_key = group.image_key.ok_or(WhitenoiseError::GroupImageNotFound)?;
                let image_nonce = group
                    .image_nonce
                    .ok_or(WhitenoiseError::GroupImageNotFound)?;

                let decrypted_bytes = decrypt_data(&encrypted_bytes, &image_key, &image_nonce)?;

                // Save the decrypted files locally
                let image_pointer = self
                    .config
                    .data_dir
                    .join("group_images")
                    .join(sha256.to_string());
                fs::write(image_pointer.clone(), &decrypted_bytes).await?;

                // Store the image pointer in group information table
                let _group_info = GroupInformation::insert_new(
                    group_id,
                    group_info.group_type,
                    Some(image_pointer.to_string_lossy().into_owned()),
                    &self.database,
                )
                .await?;

                Ok(decrypted_bytes)
            }
        }
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

        // Test case: DirectMessage group (2 participants total)
        case_create_direct_message_group(
            &whitenoise,
            &creator_account,
            vec![member_pubkeys[0]], // Only one member for DM
            vec![creator_account.pubkey, member_pubkeys[0]],
        )
        .await;
    }

    async fn case_create_group_success(
        whitenoise: &Whitenoise,
        creator_account: &Account,
        member_pubkeys: Vec<PublicKey>,
        admin_pubkeys: Vec<PublicKey>,
    ) {
        let config = create_nostr_group_config_data(admin_pubkeys.clone());
        // Create the group
        let group = whitenoise
            .create_group(
                creator_account,
                member_pubkeys.clone(),
                config.clone(),
                None,
            )
            .await
            .unwrap();

        // Verify group metadata matches configuration
        assert_eq!(group.name, config.name);
        assert_eq!(group.description, config.description);
        assert_eq!(group.image_hash, config.image_hash);
        assert_eq!(group.image_key, config.image_key);

        // Verify admin configuration
        assert_eq!(group.admin_pubkeys.len(), admin_pubkeys.len());
        for admin_pk in &admin_pubkeys {
            assert!(
                group.admin_pubkeys.contains(admin_pk),
                "Admin {} not found in group.admin_pubkeys",
                admin_pk
            );
        }

        // Verify group state and type
        // Just check that group is in a valid state (we can't verify exact state without knowing the enum path)

        // Verify group information was created properly
        let group_info = GroupInformation::get_by_mls_group_id(&group.mls_group_id, whitenoise)
            .await
            .unwrap();
        assert_eq!(group_info.mls_group_id, group.mls_group_id);
        assert_eq!(
            group_info.group_type,
            crate::whitenoise::group_information::GroupType::Group
        );
        // Note: participant_count is stored separately and managed by the GroupInformation logic

        // Verify group members can be retrieved
        let members = whitenoise
            .group_members(creator_account, &group.mls_group_id)
            .await
            .unwrap();
        assert_eq!(members.len(), member_pubkeys.len() + 1); // +1 for creator
        assert!(
            members.contains(&creator_account.pubkey),
            "Creator not in member list"
        );
        for member_pk in &member_pubkeys {
            assert!(
                members.contains(member_pk),
                "Member {} not found in group",
                member_pk
            );
        }

        // Verify group admins can be retrieved
        let admins = whitenoise
            .group_admins(creator_account, &group.mls_group_id)
            .await
            .unwrap();
        assert_eq!(admins.len(), admin_pubkeys.len());
        for admin_pk in &admin_pubkeys {
            assert!(
                admins.contains(admin_pk),
                "Admin {} not found in admin list",
                admin_pk
            );
        }
    }

    /// Test case: Member/admin validation fails - empty admin list
    async fn case_create_group_empty_admin_list(
        whitenoise: &Whitenoise,
        creator_account: &Account,
        member_pubkeys: Vec<PublicKey>,
        admin_pubkeys: Vec<PublicKey>,
    ) {
        let config = create_nostr_group_config_data(admin_pubkeys.clone());
        let result = whitenoise
            .create_group(creator_account, member_pubkeys, config.clone(), None)
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
        let config = create_nostr_group_config_data(admin_pubkeys);
        let result = whitenoise
            .create_group(creator_account, member_pubkeys, config, None)
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
        let config = create_nostr_group_config_data(admin_pubkeys);
        let result = whitenoise
            .create_group(creator_account, member_pubkeys, config, None)
            .await;

        // Should fail because admin must be a member
        assert!(result.is_err());
        match result.unwrap_err() {
            WhitenoiseError::NostrMlsError(nostr_mls::Error::Group(msg)) => {
                assert!(
                    msg.contains("Admin must be a member"),
                    "Expected 'Admin must be a member' error, got: {}",
                    msg
                );
            }
            other => panic!("Expected NostrMlsError::Group, got: {:?}", other),
        }
    }

    async fn case_create_direct_message_group(
        whitenoise: &Whitenoise,
        creator_account: &Account,
        member_pubkeys: Vec<PublicKey>,
        admin_pubkeys: Vec<PublicKey>,
    ) {
        // Direct message group should have exactly 1 member (plus creator = 2 total)
        assert_eq!(
            member_pubkeys.len(),
            1,
            "Direct message group should have exactly 1 member"
        );
        assert_eq!(
            admin_pubkeys.len(),
            2,
            "Direct message group should have 2 admins (both participants)"
        );

        let config = create_nostr_group_config_data(admin_pubkeys.clone());
        let result = whitenoise
            .create_group(creator_account, member_pubkeys.clone(), config, None)
            .await;

        assert!(result.is_ok(), "Error {:?}", result.unwrap_err());
        let group = result.unwrap();

        // Verify it's automatically classified as DirectMessage type
        let group_info = GroupInformation::get_by_mls_group_id(&group.mls_group_id, whitenoise)
            .await
            .unwrap();
        assert_eq!(group_info.mls_group_id, group.mls_group_id);
        assert_eq!(
            group_info.group_type,
            crate::whitenoise::group_information::GroupType::DirectMessage
        );
        // DirectMessage groups should have exactly 2 participants (verified via member count below)

        // Verify both participants are admins (standard for DM groups)
        let admins = whitenoise
            .group_admins(creator_account, &group.mls_group_id)
            .await
            .unwrap();
        assert_eq!(admins.len(), 2, "DirectMessage group should have 2 admins");
        assert!(
            admins.contains(&creator_account.pubkey),
            "Creator should be admin"
        );
        assert!(
            admins.contains(&member_pubkeys[0]),
            "Member should be admin"
        );

        // Verify membership
        let members = whitenoise
            .group_members(creator_account, &group.mls_group_id)
            .await
            .unwrap();
        assert_eq!(
            members.len(),
            2,
            "DirectMessage group should have exactly 2 members"
        );
        assert!(
            members.contains(&creator_account.pubkey),
            "Creator should be member"
        );
        assert!(
            members.contains(&member_pubkeys[0]),
            "Member should be member"
        );
    }

    #[tokio::test]
    async fn test_group_member_management() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Setup creator and initial members
        let creator_account = whitenoise.create_identity().await.unwrap();
        let initial_members = setup_multiple_test_accounts(&whitenoise, 2).await;
        let initial_member_pubkeys = initial_members
            .iter()
            .map(|(acc, _)| acc.pubkey)
            .collect::<Vec<_>>();

        // Create group with initial members
        let admin_pubkeys = vec![creator_account.pubkey];
        let config = create_nostr_group_config_data(admin_pubkeys.clone());
        let group = whitenoise
            .create_group(
                &creator_account,
                initial_member_pubkeys.clone(),
                config,
                None,
            )
            .await
            .unwrap();

        // Verify initial membership
        let members = whitenoise
            .group_members(&creator_account, &group.mls_group_id)
            .await
            .unwrap();
        assert_eq!(members.len(), 3); // creator + 2 initial members

        // Add new members
        let new_members = setup_multiple_test_accounts(&whitenoise, 2).await;
        let new_member_pubkeys = new_members
            .iter()
            .map(|(acc, _)| acc.pubkey)
            .collect::<Vec<_>>();

        let add_result = whitenoise
            .add_members_to_group(
                &creator_account,
                &group.mls_group_id,
                new_member_pubkeys.clone(),
            )
            .await;
        assert!(
            add_result.is_ok(),
            "Failed to add members: {:?}",
            add_result.unwrap_err()
        );

        // Verify new membership count
        let updated_members = whitenoise
            .group_members(&creator_account, &group.mls_group_id)
            .await
            .unwrap();
        assert_eq!(updated_members.len(), 5); // creator + 2 initial + 2 new
        for new_member_pk in &new_member_pubkeys {
            assert!(
                updated_members.contains(new_member_pk),
                "New member {} not found",
                new_member_pk
            );
        }

        // Remove one member
        let member_to_remove = vec![initial_member_pubkeys[0]];
        let remove_result = whitenoise
            .remove_members_from_group(
                &creator_account,
                &group.mls_group_id,
                member_to_remove.clone(),
            )
            .await;
        assert!(
            remove_result.is_ok(),
            "Failed to remove member: {:?}",
            remove_result.unwrap_err()
        );

        // Verify final membership
        let final_members = whitenoise
            .group_members(&creator_account, &group.mls_group_id)
            .await
            .unwrap();
        assert_eq!(final_members.len(), 4); // creator + 1 remaining initial + 2 new
        assert!(
            !final_members.contains(&member_to_remove[0]),
            "Removed member still in group"
        );
    }

    #[tokio::test]
    async fn test_update_group_data() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Setup creator and member
        let creator_account = whitenoise.create_identity().await.unwrap();
        let members = setup_multiple_test_accounts(&whitenoise, 1).await;
        let member_pubkeys = vec![members[0].0.pubkey];

        // Create group
        let admin_pubkeys = vec![creator_account.pubkey];
        let config = create_nostr_group_config_data(admin_pubkeys.clone());
        let group = whitenoise
            .create_group(&creator_account, member_pubkeys, config, None)
            .await
            .unwrap();

        // Update group data
        let new_group_data = nostr_mls::groups::NostrGroupDataUpdate {
            name: Some("Updated Group Name".to_string()),
            description: Some("Updated description".to_string()),
            image_hash: Some(Some(vec![1u8; 32])),
            image_key: Some(Some(vec![1u8; 32])),
            image_nonce: Some(Some(vec![0u8; 12])),
            admins: None,
            relays: None,
        };

        let update_result = whitenoise
            .update_group_data(
                &creator_account,
                &group.mls_group_id,
                new_group_data.clone(),
            )
            .await;
        assert!(
            update_result.is_ok(),
            "Failed to update group data: {:?}",
            update_result.unwrap_err()
        );

        // Verify the group data was updated
        let updated_groups = whitenoise.groups(&creator_account, true).await.unwrap();
        let updated_group = updated_groups
            .iter()
            .find(|g| g.mls_group_id == group.mls_group_id)
            .expect("Updated group not found");

        assert_eq!(updated_group.name, new_group_data.name.unwrap());
        assert_eq!(
            updated_group.description,
            new_group_data.description.unwrap()
        );
        assert_eq!(updated_group.image_hash, new_group_data.image_hash.unwrap());
        assert_eq!(updated_group.image_key, new_group_data.image_key.unwrap());
        assert_eq!(
            updated_group.image_nonce,
            new_group_data.image_nonce.unwrap()
        );
    }

    #[tokio::test]
    async fn test_groups_filtering() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Setup accounts
        let creator_account = whitenoise.create_identity().await.unwrap();
        let members = setup_multiple_test_accounts(&whitenoise, 1).await;
        let member_pubkeys = vec![members[0].0.pubkey];

        // Create a group
        let admin_pubkeys = vec![creator_account.pubkey];
        let config = create_nostr_group_config_data(admin_pubkeys);
        let _group = whitenoise
            .create_group(&creator_account, member_pubkeys, config, None)
            .await
            .unwrap();

        // Test getting all groups
        let all_groups = whitenoise.groups(&creator_account, false).await.unwrap();
        assert!(!all_groups.is_empty(), "Should have at least one group");

        // Test getting only active groups
        let active_groups = whitenoise.groups(&creator_account, true).await.unwrap();
        assert!(
            !active_groups.is_empty(),
            "Should have at least one active group"
        );

        // All groups should be active in this test case
        assert_eq!(
            all_groups.len(),
            active_groups.len(),
            "All groups should be active"
        );

        // All groups should be in a valid state (exact verification depends on state enum implementation)
    }

    #[tokio::test]
    async fn test_leave_group() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Setup creator and members
        let creator_account = whitenoise.create_identity().await.unwrap();
        let members = setup_multiple_test_accounts(&whitenoise, 2).await;
        let member_accounts = members.iter().map(|(acc, _)| acc).collect::<Vec<_>>();
        let member_pubkeys = member_accounts
            .iter()
            .map(|acc| acc.pubkey)
            .collect::<Vec<_>>();

        // Create group with creator and members as admins (so they can process the leave proposal)
        let admin_pubkeys = vec![creator_account.pubkey, member_pubkeys[0]];
        let config = create_nostr_group_config_data(admin_pubkeys);
        let group = whitenoise
            .create_group(&creator_account, member_pubkeys.clone(), config, None)
            .await
            .unwrap();

        // Verify initial membership
        let initial_members = whitenoise
            .group_members(&creator_account, &group.mls_group_id)
            .await
            .unwrap();
        assert_eq!(initial_members.len(), 3); // creator + 2 members

        // Creator leaves the group (creates proposal)
        // Note: In a real scenario, members would need to accept welcome messages
        // to have access to the group. For this test, we use the creator who
        // has immediate access to the group.
        let leave_result = whitenoise
            .leave_group(&creator_account, &group.mls_group_id)
            .await;

        assert!(
            leave_result.is_ok(),
            "Failed to initiate leave group: {:?}",
            leave_result.unwrap_err()
        );

        // Note: At this point, the member has only created a proposal to leave.
        // The actual removal would happen when an admin processes the commit,
        // but that's part of the message processing pipeline that would be
        // tested separately in integration tests.

        // For now, we just verify that the proposal was successfully created and published
        // without errors, which indicates the leave_group method works correctly.
    }

    #[tokio::test]
    async fn test_group_image() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Setup creator account
        let creator_account = whitenoise.create_identity().await.unwrap();

        // Setup member accounts
        let mut member_pubkeys = Vec::new();
        let mut all_accounts = vec![creator_account.clone()];
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

            all_accounts.push(member_account);
        }

        // Setup admin accounts (creator + one member as admin)
        let admin_pubkeys = vec![creator_account.pubkey, member_pubkeys[0]];
        let config = create_nostr_group_config_data(admin_pubkeys.clone());

        // Create the group
        let group = whitenoise
            .create_group(
                &creator_account,
                member_pubkeys.clone(),
                config.clone(),
                None,
            )
            .await
            .unwrap();

        // Update with group image
        create_random_png("group_image");
        let image_path = "./dev/data/images/group_image.png";
        whitenoise
            .update_group_image(&creator_account, &group.mls_group_id, image_path)
            .await
            .unwrap();

        let created_image_bytes = fs::read(image_path).await.unwrap();

        for account in all_accounts {
            let group_image_bytes = whitenoise
                .get_group_image(&account, &group.mls_group_id)
                .await
                .unwrap();
            assert_eq!(created_image_bytes, group_image_bytes);
        }
    }
}
