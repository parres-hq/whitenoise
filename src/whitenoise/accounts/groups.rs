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
        let serialized_welcome_message: Vec<u8>;
        let group_ids: Vec<String>;
        let mut eventid_keypackage_list: Vec<(EventId, KeyPackage)> = Vec::new();

        let nostr_mls_guard = creator_account.nostr_mls.lock().await;

        if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
            // Fetch key packages for all members
            for pk in member_pubkeys.iter() {
                let some_event = self.fetch_key_package_event(*pk).await?;
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
