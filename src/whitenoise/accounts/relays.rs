use crate::whitenoise::error::Result;
use crate::whitenoise::Whitenoise;
use crate::{whitenoise::accounts::Account, WhitenoiseError};
use dashmap::DashSet;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Copy)]
pub enum RelayType {
    Nostr,
    Inbox,
    KeyPackage,
}

impl From<String> for RelayType {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "nostr" => Self::Nostr,
            "inbox" => Self::Inbox,
            "key_package" => Self::KeyPackage,
            _ => panic!("Invalid relay type: {}", s),
        }
    }
}

impl From<RelayType> for String {
    fn from(relay_type: RelayType) -> Self {
        match relay_type {
            RelayType::Nostr => "nostr".to_string(),
            RelayType::Inbox => "inbox".to_string(),
            RelayType::KeyPackage => "key_package".to_string(),
        }
    }
}

impl From<RelayType> for Kind {
    fn from(relay_type: RelayType) -> Self {
        match relay_type {
            RelayType::Nostr => Kind::RelayList,
            RelayType::Inbox => Kind::InboxRelays,
            RelayType::KeyPackage => Kind::MlsKeyPackageRelays,
        }
    }
}

impl Whitenoise {
    /// Loads the Nostr relays associated with a user's public key.
    ///
    /// This method queries the Nostr network for relay URLs that the user has published
    /// for a specific relay type (e.g., read relays, write relays). These relays are
    /// used to determine where to send and receive Nostr events for the user.
    ///
    /// # Arguments
    ///
    /// * `pubkey` - The `PublicKey` of the user whose relays should be fetched.
    /// * `relay_type` - The type of relays to fetch (e.g., read, write, or both).
    ///
    /// # Returns
    ///
    /// Returns `Ok(DashSet<RelayUrl>)` containing the list of relay URLs, or an error if the query fails.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if the relay query fails.
    pub async fn fetch_relays_from(
        &self,
        nip65_relays: DashSet<RelayUrl>,
        pubkey: PublicKey,
        relay_type: RelayType,
    ) -> Result<DashSet<RelayUrl>> {
        let relays = self
            .nostr
            .fetch_user_relays(pubkey, relay_type, nip65_relays)
            .await?;
        Ok(relays)
    }

    /// Fetches the status of relays associated with a user's public key.
    ///
    /// This method returns a list of relay statuses for relays that are configured
    /// for the given account. It gets the relay URLs from the user's relay lists
    /// and then returns the current connection status from the Nostr client.
    ///
    /// # Arguments
    ///
    /// * `pubkey` - The `PublicKey` of the user whose relay statuses should be fetched.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Vec<(RelayUrl, RelayStatus)>)` containing relay URLs and their current
    /// status from the nostr-sdk, or an error if the query fails.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if the relay query fails.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let pubkey = PublicKey::from_hex("...").unwrap();
    /// let relay_statuses = whitenoise.fetch_relay_status(pubkey).await?;
    ///
    /// for (url, status) in relay_statuses {
    ///     println!("Relay {} status: {:?}", url, status);
    /// }
    /// ```
    pub async fn fetch_relay_status(
        &self,
        account: &Account,
    ) -> Result<Vec<(RelayUrl, RelayStatus)>> {
        // Get all relay URLs for this user across all types
        // Combine all relay URLs into one list, removing duplicates
        let mut all_relays = Vec::new();
        all_relays.extend(account.nip65_relays.clone());
        all_relays.extend(account.inbox_relays.clone());
        all_relays.extend(account.key_package_relays.clone());

        // Get current relay statuses from the Nostr client
        let mut relay_statuses = Vec::new();

        for relay_url in all_relays {
            // Try to get relay status from NostrManager
            match self.nostr.get_relay_status(&relay_url).await {
                Ok(status) => {
                    relay_statuses.push((relay_url, status));
                }
                Err(_) => {
                    // If we can't get the relay status, it's likely not connected
                    relay_statuses.push((relay_url, RelayStatus::Disconnected));
                }
            }
        }

        Ok(relay_statuses)
    }

    pub(crate) async fn connect_account_relays(&self, account: &Account) -> Result<()> {
        for relay in account
            .nip65_relays
            .iter()
            .chain(account.inbox_relays.iter())
        {
            self.nostr.client.add_relay(relay.clone()).await?;
        }

        tracing::debug!("Connecting to the account relays added");
        tokio::spawn({
            let client = self.nostr.client.clone();
            async move {
                client.connect().await;
            }
        });

        Ok(())
    }

    pub async fn add_relay_to_account(
        &self,
        pubkey: PublicKey,
        relay: RelayUrl,
        relay_type: RelayType,
    ) -> Result<()> {
        if !self.logged_in(&pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        let account = self.get_account(&pubkey).await?;

        // Add the relay to the appropriate relay set
        match relay_type {
            RelayType::Nostr => {
                account.nip65_relays.insert(relay.clone());
            }
            RelayType::Inbox => {
                account.inbox_relays.insert(relay.clone());
            }
            RelayType::KeyPackage => {
                account.key_package_relays.insert(relay.clone());
            }
        }

        // Save the updated account
        self.save_account(&account).await?;

        // Update the in-memory account
        {
            let mut accounts = self.write_accounts().await;
            accounts.insert(account.pubkey, account.clone());
        }

        // Ensure relays are connected
        self.nostr
            .ensure_relays_connected(DashSet::from_iter([relay]))
            .await?;

        self.publish_relay_list_for_account(&account, relay_type, &None)
            .await?;

        Ok(())
    }

    pub async fn remove_relay_from_account(
        &self,
        pubkey: PublicKey,
        relay: RelayUrl,
        relay_type: RelayType,
    ) -> Result<()> {
        if !self.logged_in(&pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        let account = self.get_account(&pubkey).await?;

        let current_relays = match relay_type {
            RelayType::Nostr => account.nip65_relays.clone(),
            RelayType::Inbox => account.inbox_relays.clone(),
            RelayType::KeyPackage => account.key_package_relays.clone(),
        };

        if !current_relays.contains(&relay) {
            return Ok(());
        }

        match relay_type {
            RelayType::Nostr => {
                account.nip65_relays.remove(&relay);
            }
            RelayType::Inbox => {
                account.inbox_relays.remove(&relay);
            }
            RelayType::KeyPackage => {
                account.key_package_relays.remove(&relay);
            }
        }

        self.save_account(&account).await?;

        // Update the in-memory account
        {
            let mut accounts = self.write_accounts().await;
            accounts.insert(account.pubkey, account.clone());
        }

        // TODO: Do we need to manually disconnect or just wait for next session when we won't connect?

        // We provide the prior relays to ensure we overwrite the relay list event on the relay we're leaving with the correct relays
        self.publish_relay_list_for_account(&account, relay_type, &Some(current_relays))
            .await?;

        Ok(())
    }

    pub(crate) async fn publish_account_relay_info(&self, account: &Account) -> Result<()> {
        if !self.logged_in(&account.pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        self.publish_relay_list_for_account(account, RelayType::Nostr, &None)
            .await?;
        self.publish_relay_list_for_account(account, RelayType::Inbox, &None)
            .await?;
        self.publish_relay_list_for_account(account, RelayType::KeyPackage, &None)
            .await
    }

    pub(crate) async fn publish_relay_list_for_account(
        &self,
        account: &Account,
        relay_type: RelayType,
        target_relays: &Option<DashSet<RelayUrl>>, // If provided, this means at least one relay was removed. We need to publish to the prior relays as well.
    ) -> Result<()> {
        // Determine the kind of relay list event to publish
        let (relay_event_kind, relays_to_publish) = match relay_type {
            RelayType::Nostr => (Kind::RelayList, account.nip65_relays.clone()),
            RelayType::Inbox => (Kind::InboxRelays, account.inbox_relays.clone()),
            RelayType::KeyPackage => (
                Kind::MlsKeyPackageRelays,
                account.key_package_relays.clone(),
            ),
        };

        let relays_to_use = match target_relays.as_ref() {
            Some(relays) => relays,
            None => &account.nip65_relays,
        };

        // Create a minimal relay list event
        let tags: Vec<Tag> = relays_to_publish
            .clone()
            .into_iter()
            .map(|url| Tag::custom(TagKind::Relay, [url.to_string()]))
            .collect();
        tracing::debug!("Publishing relay list tags {:?}", tags);

        let event = EventBuilder::new(relay_event_kind, "").tags(tags);
        let keys = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?;

        let result = self
            .nostr
            .publish_event_builder_with_signer(event.clone(), relays_to_use.clone(), keys)
            .await?;
        tracing::debug!(target: "whitenoise::publish_relay_list", "Published relay list event to Nostr: {:?}", result);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use nostr::types::RelayUrl;
    use tokio::time::sleep;

    use crate::whitenoise::test_utils::create_mock_whitenoise;
    use crate::{RelayType, Whitenoise};

    #[tokio::test]
    async fn test_add_remove_relay_comprehensive() {
        let test_relay = RelayUrl::parse("ws://localhost:6666").unwrap();
        let test_relay2 = RelayUrl::parse("ws://localhost:9999").unwrap();
        let (whitenoise, _, _) = create_mock_whitenoise().await;
        let account = whitenoise.create_identity().await.unwrap();

        tracing::info!("Testing comprehensive relay operations");

        // Test Nostr (NIP-65) relays
        await_test_relay_type(
            &whitenoise,
            &account.pubkey,
            RelayType::Nostr,
            test_relay.clone(),
            test_relay2.clone(),
        )
        .await;

        // Test Inbox relays
        await_test_relay_type(
            &whitenoise,
            &account.pubkey,
            RelayType::Inbox,
            test_relay.clone(),
            test_relay2.clone(),
        )
        .await;

        // Test Key Package relays
        await_test_relay_type(
            &whitenoise,
            &account.pubkey,
            RelayType::KeyPackage,
            test_relay.clone(),
            test_relay2.clone(),
        )
        .await;
    }

    async fn await_test_relay_type(
        whitenoise: &Whitenoise,
        pubkey: &nostr_sdk::PublicKey,
        relay_type: RelayType,
        test_relay: RelayUrl,
        test_relay2: RelayUrl,
    ) {
        let relay_type_name = match relay_type {
            RelayType::Nostr => "Nostr",
            RelayType::Inbox => "Inbox",
            RelayType::KeyPackage => "KeyPackage",
        };

        tracing::info!("Testing {} relay operations", relay_type_name);

        // Get initial state
        let initial_account = whitenoise.get_account(pubkey).await.unwrap();
        let initial_count = match relay_type {
            RelayType::Nostr => initial_account.nip65_relays.len(),
            RelayType::Inbox => initial_account.inbox_relays.len(),
            RelayType::KeyPackage => initial_account.key_package_relays.len(),
        };

        // Test adding a relay
        tracing::info!("Adding {} relay", relay_type_name);
        whitenoise
            .add_relay_to_account(*pubkey, test_relay.clone(), relay_type)
            .await
            .unwrap();

        // Verify in-memory account state after add
        let updated_account = whitenoise.get_account(pubkey).await.unwrap();
        let current_relays = match relay_type {
            RelayType::Nostr => &updated_account.nip65_relays,
            RelayType::Inbox => &updated_account.inbox_relays,
            RelayType::KeyPackage => &updated_account.key_package_relays,
        };
        assert!(
            current_relays.contains(&test_relay),
            "{} relay should be present in in-memory account after add",
            relay_type_name
        );
        assert_eq!(
            current_relays.len(),
            initial_count + 1,
            "{} relay count should increase by 1",
            relay_type_name
        );

        // Verify database state after add
        let db_relays = whitenoise
            .get_account_relays_db(pubkey, relay_type)
            .await
            .unwrap();
        assert!(
            db_relays.contains(&test_relay),
            "{} relay should be present in database after add",
            relay_type_name
        );
        assert!(
            Whitenoise::relayurl_dashset_eq(db_relays.clone(), current_relays.clone()),
            "{} relays should match between in-memory and database",
            relay_type_name
        );

        sleep(Duration::from_millis(100)).await;

        // Test adding duplicate relay (should not increase count)
        tracing::info!("Testing duplicate {} relay add", relay_type_name);
        whitenoise
            .add_relay_to_account(*pubkey, test_relay.clone(), relay_type)
            .await
            .unwrap();

        let after_duplicate_account = whitenoise.get_account(pubkey).await.unwrap();
        let after_duplicate_relays = match relay_type {
            RelayType::Nostr => &after_duplicate_account.nip65_relays,
            RelayType::Inbox => &after_duplicate_account.inbox_relays,
            RelayType::KeyPackage => &after_duplicate_account.key_package_relays,
        };
        assert_eq!(
            after_duplicate_relays.len(),
            initial_count + 1,
            "{} relay count should not increase for duplicate add",
            relay_type_name
        );

        sleep(Duration::from_millis(100)).await;

        // Test adding a second relay
        tracing::info!("Adding second {} relay", relay_type_name);
        whitenoise
            .add_relay_to_account(*pubkey, test_relay2.clone(), relay_type)
            .await
            .unwrap();

        let two_relay_account = whitenoise.get_account(pubkey).await.unwrap();
        let two_relay_relays = match relay_type {
            RelayType::Nostr => &two_relay_account.nip65_relays,
            RelayType::Inbox => &two_relay_account.inbox_relays,
            RelayType::KeyPackage => &two_relay_account.key_package_relays,
        };
        assert!(
            two_relay_relays.contains(&test_relay2),
            "Second {} relay should be present",
            relay_type_name
        );
        assert_eq!(
            two_relay_relays.len(),
            initial_count + 2,
            "{} relay count should be initial + 2",
            relay_type_name
        );

        sleep(Duration::from_millis(100)).await;

        // Test removing first relay
        tracing::info!("Removing first {} relay", relay_type_name);
        whitenoise
            .remove_relay_from_account(*pubkey, test_relay.clone(), relay_type)
            .await
            .unwrap();

        // Verify in-memory account state after remove
        let after_remove_account = whitenoise.get_account(pubkey).await.unwrap();
        let after_remove_relays = match relay_type {
            RelayType::Nostr => &after_remove_account.nip65_relays,
            RelayType::Inbox => &after_remove_account.inbox_relays,
            RelayType::KeyPackage => &after_remove_account.key_package_relays,
        };
        assert!(
            !after_remove_relays.contains(&test_relay),
            "{} relay should be removed from in-memory account",
            relay_type_name
        );
        assert!(
            after_remove_relays.contains(&test_relay2),
            "Second {} relay should remain",
            relay_type_name
        );
        assert_eq!(
            after_remove_relays.len(),
            initial_count + 1,
            "{} relay count should be initial + 1 after remove",
            relay_type_name
        );

        // Verify database state after remove
        let db_relays_after_remove = whitenoise
            .get_account_relays_db(pubkey, relay_type)
            .await
            .unwrap();
        assert!(
            !db_relays_after_remove.contains(&test_relay),
            "{} relay should be removed from database",
            relay_type_name
        );
        assert!(
            Whitenoise::relayurl_dashset_eq(db_relays_after_remove, after_remove_relays.clone()),
            "{} relays should match between in-memory and database after remove",
            relay_type_name
        );

        sleep(Duration::from_millis(100)).await;

        // Test removing non-existent relay (should be no-op)
        tracing::info!("Testing removal of non-existent {} relay", relay_type_name);
        let non_existent_relay = RelayUrl::parse("ws://localhost:1234").unwrap();
        whitenoise
            .remove_relay_from_account(*pubkey, non_existent_relay, relay_type)
            .await
            .unwrap();

        let final_account = whitenoise.get_account(pubkey).await.unwrap();
        let final_relays = match relay_type {
            RelayType::Nostr => &final_account.nip65_relays,
            RelayType::Inbox => &final_account.inbox_relays,
            RelayType::KeyPackage => &final_account.key_package_relays,
        };
        assert_eq!(
            final_relays.len(),
            initial_count + 1,
            "{} relay count should remain unchanged after removing non-existent relay",
            relay_type_name
        );

        tracing::info!("{} relay tests completed successfully", relay_type_name);
    }

    #[tokio::test]
    async fn test_convenience_methods() {
        let test_relay = RelayUrl::parse("ws://localhost:8888").unwrap();
        let (whitenoise, _, _) = create_mock_whitenoise().await;
        let account = whitenoise.create_identity().await.unwrap();

        // Test convenience methods for adding relays
        whitenoise
            .add_relay_to_account(account.pubkey, test_relay.clone(), RelayType::Nostr)
            .await
            .unwrap();
        whitenoise
            .add_relay_to_account(account.pubkey, test_relay.clone(), RelayType::Inbox)
            .await
            .unwrap();
        whitenoise
            .add_relay_to_account(account.pubkey, test_relay.clone(), RelayType::KeyPackage)
            .await
            .unwrap();

        // Verify all relay types contain the test relay
        let updated_account = whitenoise.get_account(&account.pubkey).await.unwrap();
        assert!(updated_account.nip65_relays.contains(&test_relay));
        assert!(updated_account.inbox_relays.contains(&test_relay));
        assert!(updated_account.key_package_relays.contains(&test_relay));

        // Test convenience methods for removing relays
        whitenoise
            .remove_relay_from_account(account.pubkey, test_relay.clone(), RelayType::Nostr)
            .await
            .unwrap();
        whitenoise
            .remove_relay_from_account(account.pubkey, test_relay.clone(), RelayType::Inbox)
            .await
            .unwrap();
        whitenoise
            .remove_relay_from_account(account.pubkey, test_relay.clone(), RelayType::KeyPackage)
            .await
            .unwrap();

        // Verify relay was removed from all types (should not be in the added relay anymore,
        // but might still be in default relays)
        let final_account = whitenoise.get_account(&account.pubkey).await.unwrap();
        // Since test_relay is not in default relays, it should be completely removed
        assert!(!final_account.nip65_relays.contains(&test_relay));
        assert!(!final_account.inbox_relays.contains(&test_relay));
        assert!(!final_account.key_package_relays.contains(&test_relay));
    }

    #[tokio::test]
    async fn test_fetch_relay_status() {
        let (whitenoise, _, _) = create_mock_whitenoise().await;
        let account = whitenoise.create_identity().await.unwrap();

        // Test that we can fetch relay status without errors
        let relay_statuses = whitenoise.fetch_relay_status(&account).await.unwrap();

        // Should have statuses for all relay types
        assert!(!relay_statuses.is_empty());

        // Each relay should have a status (even if disconnected)
        for (relay_url, status) in relay_statuses {
            tracing::info!("Relay {} has status: {:?}", relay_url, status);
            // Just verify we got some status - actual connection testing would require running relays
        }
    }

    #[tokio::test]
    async fn test_edge_cases() {
        let (whitenoise, _, _) = create_mock_whitenoise().await;
        let account = whitenoise.create_identity().await.unwrap();

        // Test adding relay to non-existent account should fail
        let fake_pubkey = nostr_sdk::Keys::generate().public_key();
        let test_relay = RelayUrl::parse("ws://localhost:7777").unwrap();

        let result = whitenoise
            .add_relay_to_account(fake_pubkey, test_relay.clone(), RelayType::Nostr)
            .await;
        assert!(
            result.is_err(),
            "Adding relay to non-existent account should fail"
        );

        // Test removing relay from non-existent account should fail
        let result = whitenoise
            .remove_relay_from_account(fake_pubkey, test_relay.clone(), RelayType::Nostr)
            .await;
        assert!(
            result.is_err(),
            "Removing relay from non-existent account should fail"
        );

        // Test removing relay that doesn't exist in account (should succeed as no-op)
        let non_existent_relay = RelayUrl::parse("ws://localhost:9876").unwrap();
        let result = whitenoise
            .remove_relay_from_account(account.pubkey, non_existent_relay, RelayType::Nostr)
            .await;
        assert!(
            result.is_ok(),
            "Removing non-existent relay should succeed as no-op"
        );

        // Test that account state is unchanged after removing non-existent relay
        let initial_account = whitenoise.get_account(&account.pubkey).await.unwrap();
        let initial_count = initial_account.nip65_relays.len();

        let after_remove_account = whitenoise.get_account(&account.pubkey).await.unwrap();
        assert_eq!(
            after_remove_account.nip65_relays.len(),
            initial_count,
            "Account relay count should be unchanged after removing non-existent relay"
        );
    }

    #[tokio::test]
    async fn test_relay_type_conversions() {
        use crate::RelayType;
        use nostr_sdk::Kind;

        // Test String conversions
        assert_eq!(String::from(RelayType::Nostr), "nostr");
        assert_eq!(String::from(RelayType::Inbox), "inbox");
        assert_eq!(String::from(RelayType::KeyPackage), "key_package");

        // Test Kind conversions
        assert_eq!(Kind::from(RelayType::Nostr), Kind::RelayList);
        assert_eq!(Kind::from(RelayType::Inbox), Kind::InboxRelays);
        assert_eq!(Kind::from(RelayType::KeyPackage), Kind::MlsKeyPackageRelays);

        // Test from String conversions
        assert_eq!(RelayType::from("nostr".to_string()), RelayType::Nostr);
        assert_eq!(RelayType::from("inbox".to_string()), RelayType::Inbox);
        assert_eq!(
            RelayType::from("key_package".to_string()),
            RelayType::KeyPackage
        );

        // Test case insensitive
        assert_eq!(RelayType::from("NOSTR".to_string()), RelayType::Nostr);
        assert_eq!(RelayType::from("InBoX".to_string()), RelayType::Inbox);
    }

    #[tokio::test]
    #[should_panic(expected = "Invalid relay type")]
    async fn test_invalid_relay_type_conversion() {
        let _ = RelayType::from("invalid".to_string());
    }

    #[tokio::test]
    async fn test_database_consistency() {
        let (whitenoise, _, _) = create_mock_whitenoise().await;
        let account = whitenoise.create_identity().await.unwrap();
        let test_relay = RelayUrl::parse("ws://localhost:5555").unwrap();

        // Add relay and verify both in-memory and database are consistent
        whitenoise
            .add_relay_to_account(account.pubkey, test_relay.clone(), RelayType::Nostr)
            .await
            .unwrap();

        // Get fresh account from database
        let db_account = whitenoise.load_account(&account.pubkey).await.unwrap();

        // Get in-memory account
        let memory_account = whitenoise.get_account(&account.pubkey).await.unwrap();

        // Both should contain the test relay
        assert!(db_account.nip65_relays.contains(&test_relay));
        assert!(memory_account.nip65_relays.contains(&test_relay));

        // Both should have same relay count
        assert_eq!(
            db_account.nip65_relays.len(),
            memory_account.nip65_relays.len()
        );

        // Relay sets should be equivalent
        assert!(Whitenoise::relayurl_dashset_eq(
            db_account.nip65_relays.clone(),
            memory_account.nip65_relays.clone()
        ));

        // Now remove the relay and verify consistency again
        whitenoise
            .remove_relay_from_account(account.pubkey, test_relay.clone(), RelayType::Nostr)
            .await
            .unwrap();

        let db_account_after = whitenoise.load_account(&account.pubkey).await.unwrap();
        let memory_account_after = whitenoise.get_account(&account.pubkey).await.unwrap();

        // Both should not contain the test relay anymore
        assert!(!db_account_after.nip65_relays.contains(&test_relay));
        assert!(!memory_account_after.nip65_relays.contains(&test_relay));

        // Relay sets should still be equivalent
        assert!(Whitenoise::relayurl_dashset_eq(
            db_account_after.nip65_relays.clone(),
            memory_account_after.nip65_relays.clone()
        ));
    }
}
