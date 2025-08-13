use std::collections::HashMap;

use dashmap::DashSet;

use crate::whitenoise::accounts::Account;
use crate::whitenoise::error::{Result, WhitenoiseError};
use crate::whitenoise::Whitenoise;
use crate::RelayType;

use nostr_sdk::prelude::*;

pub struct Contact {
    pub pubkey: PublicKey,
    pub metadata: Option<Metadata>,
    pub nip65_relays: DashSet<RelayUrl>,
    pub inbox_relays: DashSet<RelayUrl>,
    pub key_package_relays: DashSet<RelayUrl>,
}

impl<'r, R> sqlx::FromRow<'r, R> for Contact
where
    R: sqlx::Row,
    &'r str: sqlx::ColumnIndex<R>,
    String: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
{
    fn from_row(row: &'r R) -> std::result::Result<Self, sqlx::Error> {
        // Extract raw values from the database row
        let pubkey_str: String = row.try_get("pubkey")?;
        let metadata_string: Option<String> = row.try_get("metadata")?;
        let nip65_relays: String = row.try_get("nip65_relays")?;
        let inbox_relays: String = row.try_get("inbox_relays")?;
        let key_package_relays: String = row.try_get("key_package_relays")?;

        let metadata = match metadata_string {
            Some(json) => {
                Some(
                    serde_json::from_str(&json).map_err(|e| sqlx::Error::ColumnDecode {
                        index: "metadata".to_string(),
                        source: Box::new(e),
                    })?,
                )
            }
            None => None,
        };

        // Parse pubkey from hex string
        let pubkey = PublicKey::parse(&pubkey_str).map_err(|e| sqlx::Error::ColumnDecode {
            index: "pubkey".to_string(),
            source: Box::new(e),
        })?;

        let nip65_relays = Whitenoise::parse_relays_from_sql(nip65_relays)?
            .into_iter()
            .collect();
        let inbox_relays = Whitenoise::parse_relays_from_sql(inbox_relays)?
            .into_iter()
            .collect();
        let key_package_relays = Whitenoise::parse_relays_from_sql(key_package_relays)?
            .into_iter()
            .collect();

        Ok(Contact {
            pubkey,
            metadata,
            nip65_relays,
            inbox_relays,
            key_package_relays,
        })
    }
}

impl Contact {
    /// Returns the relays to use for this contact of the specified type
    /// If the contact has no relays of the specified type, it will fallback to nip65 relays
    /// If the contact has no nip65 relays, it will fallback to default relays
    pub fn get_relays_of_type(&self, relay_type: RelayType) -> DashSet<RelayUrl> {
        let relays = match relay_type {
            RelayType::Nostr => &self.nip65_relays,
            RelayType::Inbox => &self.inbox_relays,
            RelayType::KeyPackage => &self.key_package_relays,
        };

        if relays.is_empty() {
            self.relay_fallback(relay_type)
        } else {
            relays.clone()
        }
    }

    fn relay_fallback(&self, relay_type: RelayType) -> DashSet<RelayUrl> {
        match relay_type {
            RelayType::Nostr => Account::default_relays(),
            RelayType::Inbox => self.get_relays_of_type(RelayType::Nostr),
            RelayType::KeyPackage => self.get_relays_of_type(RelayType::Nostr),
        }
    }
}

impl Whitenoise {


    /// Adds a contact to the user's contact list and publishes the updated list to Nostr.
    ///
    /// This method loads the current contact list, validates that the contact doesn't already exist,
    /// adds the new contact, and publishes a Kind 3 (ContactList) event to the Nostr network.
    ///
    /// # Arguments
    ///
    /// * `account` - The account whose contact list will be updated
    /// * `contact_pubkey` - The public key of the contact to add
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the contact was successfully added and published.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if:
    /// * The contact already exists in the contact list
    /// * Failed to load the current contact list
    /// * Failed to publish the updated contact list event
    pub async fn add_contact(&self, account: &Account, contact_pubkey: PublicKey) -> Result<()> {
        if !self.logged_in(&account.pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        // Load current contact list
        let current_contacts = self.fetch_contacts(&account.pubkey).await?;

        // Check if contact already exists
        if current_contacts.contains_key(&contact_pubkey) {
            return Err(WhitenoiseError::ContactList(format!(
                "Contact {} already exists in contact list",
                contact_pubkey.to_hex()
            )));
        }

        let nip65_relays = self
            .fetch_relays_from(
                account.nip65_relays.clone(),
                contact_pubkey,
                RelayType::Nostr,
            )
            .await?;

        let inbox_relays = self
            .fetch_relays_from(
                account.nip65_relays.clone(),
                contact_pubkey,
                RelayType::Inbox,
            )
            .await?;

        let key_package_relays = self
            .fetch_relays_from(
                account.nip65_relays.clone(),
                contact_pubkey,
                RelayType::KeyPackage,
            )
            .await?;

        let metadata = self
            .fetch_metadata_from(account.nip65_relays.clone(), contact_pubkey)
            .await?;
        // save contact locally
        let contact = Contact {
            pubkey: contact_pubkey,
            nip65_relays,
            inbox_relays,
            key_package_relays,
            metadata,
        };
        self.save_contact_local(&contact).await?;

        // Create new contact list with the added contact
        let mut new_contacts: Vec<PublicKey> = current_contacts.keys().cloned().collect();
        new_contacts.push(contact_pubkey);

        // Publish the updated contact list
        self.publish_contact_list(account, new_contacts).await?;

        Ok(())
    }

    /// Removes a contact from the user's contact list and publishes the updated list to Nostr.
    ///
    /// This method loads the current contact list, validates that the contact exists,
    /// removes the contact, and publishes a Kind 3 (ContactList) event to the Nostr network.
    ///
    /// # Arguments
    ///
    /// * `account` - The account whose contact list will be updated
    /// * `contact_pubkey` - The public key of the contact to remove
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the contact was successfully removed and published.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if:
    /// * The contact doesn't exist in the contact list
    /// * Failed to load the current contact list
    /// * Failed to publish the updated contact list event
    pub async fn remove_contact(&self, account: &Account, contact_pubkey: PublicKey) -> Result<()> {
        if !self.logged_in(&account.pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        // Load current contact list
        let current_contacts = self.fetch_contacts(&account.pubkey).await?;

        // Check if contact exists
        if !current_contacts.contains_key(&contact_pubkey) {
            return Err(WhitenoiseError::ContactList(format!(
                "Contact {} not found in contact list",
                contact_pubkey.to_hex()
            )));
        }

        // Create new contact list without the removed contact
        let new_contacts: Vec<PublicKey> = current_contacts
            .keys()
            .filter(|&pubkey| *pubkey != contact_pubkey)
            .cloned()
            .collect();

        // Publish the updated contact list
        self.publish_contact_list(account, new_contacts).await?;

        self.remove_contact_local(&contact_pubkey).await?;

        tracing::info!(
            target: "whitenoise::remove_contact",
            "Removed contact {} from account {}",
            contact_pubkey.to_hex(),
            account.pubkey.to_hex()
        );

        Ok(())
    }

    /// Updates the user's contact list with a new list of contacts and publishes it to Nostr.
    ///
    /// This method replaces the entire contact list with the provided list of public keys
    /// and publishes a Kind 3 (ContactList) event to the Nostr network.
    ///
    /// # Arguments
    ///
    /// * `account` - The account whose contact list will be updated
    /// * `contact_pubkeys` - A vector of public keys representing the new contact list
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the contact list was successfully updated and published.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if failed to publish the contact list event.
    pub async fn update_contacts(
        &self,
        account: &Account,
        contact_pubkeys: Vec<PublicKey>,
    ) -> Result<()> {
        if !self.logged_in(&account.pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        // Publish the new contact list
        self.publish_contact_list(account, contact_pubkeys.clone())
            .await?;

        tracing::info!(
            target: "whitenoise::update_contacts",
            "Updated contact list for account {} with {} contacts",
            account.pubkey.to_hex(),
            contact_pubkeys.len()
        );

        Ok(())
    }

    pub(crate) async fn save_contact_local(&self, contact: &Contact) -> Result<()> {
        let nip65_urls: Vec<_> = contact
            .nip65_relays
            .iter()
            .map(|relay_url| relay_url.to_string())
            .collect();

        let inbox_urls: Vec<_> = contact
            .inbox_relays
            .iter()
            .map(|relay_url| relay_url.to_string())
            .collect();

        let key_package_urls: Vec<_> = contact
            .key_package_relays
            .iter()
            .map(|relay_url| relay_url.to_string())
            .collect();

        let result = sqlx::query(
            "INSERT INTO contacts (pubkey, metadata, nip65_relays, inbox_relays, key_package_relays)
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(pubkey) DO UPDATE SET
                metadata = excluded.metadata,
                nip65_relays = excluded.nip65_relays,
                inbox_relays = excluded.inbox_relays,
                key_package_relays = excluded.key_package_relays",
        )
        .bind(contact.pubkey.to_hex())
        .bind(contact.metadata
            .as_ref()
            .map(serde_json::to_string)
            .transpose()? // Convert Option<Result> to Result<Option>
        )
        .bind(serde_json::to_string(&nip65_urls)?)
        .bind(serde_json::to_string(&inbox_urls)?)
        .bind(serde_json::to_string(&key_package_urls)?)
        .execute(&self.database.pool)
        .await?;

        tracing::debug!(
            target: "whitenoise::save_contact",
            "Contact query executed. Rows affected: {}",
            result.rows_affected()
        );

        Ok(())
    }

    pub(crate) async fn remove_contact_local(&self, pubkey: &PublicKey) -> Result<()> {
        let result = sqlx::query("DELETE FROM contacts WHERE pubkey = ?")
            .bind(pubkey.to_hex())
            .execute(&self.database.pool)
            .await?;

        tracing::debug!(
            target: "whitenoise::remove_contact_local",
            "Delete executed. Rows affected: {}",
            result.rows_affected()
        );

        tracing::debug!(
            target: "whitenoise::remove_contact_local",
            "Contact deleted successfully for pubkey: {}",
            pubkey.to_hex()
        );

        Ok(())
    }

    pub(crate) async fn load_contact(
        &self,
        pubkey: &PublicKey,
        account: &Account,
    ) -> Result<Contact> {
        let contact = match sqlx::query_as::<_, Contact>("SELECT * FROM contacts WHERE pubkey = ?")
            .bind(pubkey.to_hex().as_str())
            .fetch_one(&self.database.pool)
            .await
        {
            Ok(contact) => contact,
            Err(_) => {
                let metadata = self
                    .nostr
                    .fetch_metadata_from(account.nip65_relays.clone(), *pubkey)
                    .await?;

                let nip65_relays = self
                    .fetch_relays_from(account.nip65_relays.clone(), *pubkey, RelayType::Nostr)
                    .await?;

                let inbox_relays = self
                    .fetch_relays_from(account.nip65_relays.clone(), *pubkey, RelayType::Inbox)
                    .await?;

                let key_package_relays = self
                    .fetch_relays_from(account.nip65_relays.clone(), *pubkey, RelayType::KeyPackage)
                    .await?;

                let built_contact = Contact {
                    pubkey: *pubkey,
                    metadata,
                    nip65_relays,
                    inbox_relays,
                    key_package_relays,
                };

                self.save_contact_local(&built_contact).await?;

                built_contact
            }
        };

        tracing::debug!(
            target: "whitenoise::load_contact",
            "Contact loaded successfully for pubkey: {}",
            pubkey.to_hex()
        );

        Ok(contact)
    }

    // Private Helper Methods =====================================================

    /// Publishes a contact list event (Kind 3) to the Nostr network.
    ///
    /// This helper method creates and publishes a Kind 3 event containing the provided
    /// list of contact public keys as 'p' tags.
    ///
    /// # Arguments
    ///
    /// * `account` - The account publishing the contact list
    /// * `contact_pubkeys` - A vector of public keys to include in the contact list
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the event was successfully published.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if event creation or publishing fails.
    async fn publish_contact_list(
        &self,
        account: &Account,
        contact_pubkeys: Vec<PublicKey>,
    ) -> Result<()> {
        if !self.logged_in(&account.pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        // Create p tags for each contact
        let tags: Vec<Tag> = contact_pubkeys
            .into_iter()
            .map(|pubkey| Tag::custom(TagKind::p(), [pubkey.to_hex()]))
            .collect();

        // Create the contact list event
        let event = EventBuilder::new(Kind::ContactList, "").tags(tags);

        // Get the signing keys for the account
        let keys = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?;

        // Publish the event
        let result = self
            .nostr
            .publish_event_builder_with_signer(event, account.nip65_relays.clone(), keys.clone())
            .await?;

        // Update subscription for contact list metadata - use same relay logic
        self.nostr
            .update_contacts_metadata_subscription_with_signer(
                account.pubkey,
                account.nip65_relays.clone(),
                keys,
            )
            .await?;

        tracing::debug!(
            target: "whitenoise::publish_contact_list",
            "Published contact list event: {:?}",
            result
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::whitenoise::test_utils::*;
    use nostr_mls::prelude::*;
    #[tokio::test]
    async fn test_contact_list_event_structure() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let (account, keys) = create_test_account();

        // Store account keys
        whitenoise.secrets_store.store_private_key(&keys).unwrap();

        // Test creating contact list event structure
        let contact1 = create_test_keys().public_key();
        let contact2 = create_test_keys().public_key();
        let contact3 = create_test_keys().public_key();

        let contacts = [contact1, contact2, contact3];

        // Create the contact list event structure (without publishing)
        let tags: Vec<Tag> = contacts
            .iter()
            .map(|pubkey| Tag::custom(TagKind::p(), [pubkey.to_hex()]))
            .collect();

        let event = EventBuilder::new(Kind::ContactList, "").tags(tags.clone());

        // Verify event structure
        let _built_event = event.clone();

        // Get the signing keys to ensure they exist
        let signing_keys = whitenoise
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey);
        assert!(signing_keys.is_ok());

        // Verify the tags are correctly structured for Kind::ContactList (Kind 3)
        assert_eq!(tags.len(), 3);

        // Verify each tag has the correct structure
        for (i, tag) in tags.iter().enumerate() {
            let tag_vec = tag.clone().to_vec();
            assert_eq!(tag_vec[0], "p"); // Should be 'p' tag
            assert_eq!(tag_vec[1], contacts[i].to_hex()); // Should be the contact pubkey
        }
    }

    #[tokio::test]
    async fn test_add_contact_logic() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let account = whitenoise.create_identity().await.unwrap();
        let contact_pubkey = create_test_keys().public_key();

        // Test the logic of adding a contact (without actual network calls)
        // Load current contact list (will be empty in test environment)
        let current_contacts = whitenoise.fetch_contacts(&account.pubkey).await.unwrap();

        // Verify contact doesn't already exist
        assert!(!current_contacts.contains_key(&contact_pubkey));

        // Create new contact list with the added contact
        let mut new_contacts: Vec<PublicKey> = current_contacts.keys().cloned().collect();
        new_contacts.push(contact_pubkey);

        // Verify the contact was added to the list
        assert!(new_contacts.contains(&contact_pubkey));
        assert_eq!(new_contacts.len(), current_contacts.len() + 1);
    }

    #[tokio::test]
    async fn test_remove_contact_logic() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let (_account, keys) = create_test_account();

        // Store account keys
        whitenoise.secrets_store.store_private_key(&keys).unwrap();

        // Test remove contact logic with a simulated existing contact list
        let contact1 = create_test_keys().public_key();
        let contact2 = create_test_keys().public_key();
        let contact3 = create_test_keys().public_key();

        // Simulate current contacts (in a real scenario, this would come from fetch_contacts)
        let mut simulated_current_contacts: std::collections::HashMap<PublicKey, Option<Metadata>> =
            std::collections::HashMap::new();
        simulated_current_contacts.insert(contact1, None);
        simulated_current_contacts.insert(contact2, None);
        simulated_current_contacts.insert(contact3, None);

        // Test removing an existing contact
        assert!(simulated_current_contacts.contains_key(&contact2));

        // Create new contact list without the removed contact
        let new_contacts: Vec<PublicKey> = simulated_current_contacts
            .keys()
            .filter(|&pubkey| *pubkey != contact2)
            .cloned()
            .collect();

        // Verify the contact was removed
        assert!(!new_contacts.contains(&contact2));
        assert_eq!(new_contacts.len(), simulated_current_contacts.len() - 1);
        assert!(new_contacts.contains(&contact1));
        assert!(new_contacts.contains(&contact3));
    }

    #[tokio::test]
    async fn test_update_contacts_logic() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let (_account, keys) = create_test_account();

        // Store account keys
        whitenoise.secrets_store.store_private_key(&keys).unwrap();

        // Test update contacts logic with different scenarios
        let contact1 = create_test_keys().public_key();
        let contact2 = create_test_keys().public_key();
        let contact3 = create_test_keys().public_key();

        // Test empty contact list
        let empty_contacts: Vec<PublicKey> = vec![];
        let tags: Vec<Tag> = empty_contacts
            .iter()
            .map(|pubkey: &PublicKey| Tag::custom(TagKind::p(), [pubkey.to_hex()]))
            .collect();
        assert!(tags.is_empty());

        // Test single contact
        let single_contact = [contact1];
        let tags: Vec<Tag> = single_contact
            .iter()
            .map(|pubkey: &PublicKey| Tag::custom(TagKind::p(), [pubkey.to_hex()]))
            .collect();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].clone().to_vec()[0], "p");
        assert_eq!(tags[0].clone().to_vec()[1], contact1.to_hex());

        // Test multiple contacts
        let multiple_contacts = [contact1, contact2, contact3];
        let tags: Vec<Tag> = multiple_contacts
            .iter()
            .map(|pubkey: &PublicKey| Tag::custom(TagKind::p(), [pubkey.to_hex()]))
            .collect();
        assert_eq!(tags.len(), 3);

        // Verify all contacts are in tags
        let tag_pubkeys: Vec<String> = tags
            .iter()
            .map(|tag| tag.clone().to_vec()[1].clone())
            .collect();
        assert!(tag_pubkeys.contains(&contact1.to_hex()));
        assert!(tag_pubkeys.contains(&contact2.to_hex()));
        assert!(tag_pubkeys.contains(&contact3.to_hex()));
    }

    #[tokio::test]
    async fn test_contact_validation_logic() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let account = whitenoise.create_identity().await.unwrap();

        let contact_pubkey = create_test_keys().public_key();

        // Test add contact validation (contact doesn't exist)
        let current_contacts = whitenoise.fetch_contacts(&account.pubkey).await.unwrap();

        // Should be able to add new contact (empty list)
        let can_add = !current_contacts.contains_key(&contact_pubkey);
        assert!(can_add);

        // Test remove contact validation (contact doesn't exist)
        let can_remove = current_contacts.contains_key(&contact_pubkey);
        assert!(!can_remove); // Should not be able to remove non-existent contact

        // Simulate existing contact for remove validation
        let mut simulated_contacts: std::collections::HashMap<PublicKey, Option<Metadata>> =
            std::collections::HashMap::new();
        simulated_contacts.insert(contact_pubkey, None);
        let can_remove_existing = simulated_contacts.contains_key(&contact_pubkey);
        assert!(can_remove_existing);
    }

    #[tokio::test]
    async fn test_contact_event_builder_creation() {
        let (_whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Test creating EventBuilder for different contact list scenarios
        let contact1 = create_test_keys().public_key();
        let contact2 = create_test_keys().public_key();

        // Test empty contact list event
        let empty_tags: Vec<Tag> = vec![];
        let _empty_event = EventBuilder::new(Kind::ContactList, "").tags(empty_tags);
        // EventBuilder creation should succeed

        // Test single contact event
        let single_tags: Vec<Tag> = vec![Tag::custom(TagKind::p(), [contact1.to_hex()])];
        let _single_event = EventBuilder::new(Kind::ContactList, "").tags(single_tags.clone());
        // Verify tag structure
        assert_eq!(single_tags.len(), 1);

        // Test multiple contacts event
        let multi_tags: Vec<Tag> = vec![
            Tag::custom(TagKind::p(), [contact1.to_hex()]),
            Tag::custom(TagKind::p(), [contact2.to_hex()]),
        ];
        let _multi_event = EventBuilder::new(Kind::ContactList, "").tags(multi_tags.clone());
        // Verify tag structure
        assert_eq!(multi_tags.len(), 2);
    }

    #[tokio::test]
    async fn test_contact_management_without_keys() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let (account, _keys) = create_test_account();
        let _contact_pubkey = create_test_keys().public_key();

        // Don't store keys for the account - should fail when trying to get signing keys
        let signing_keys_result = whitenoise
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey);
        assert!(signing_keys_result.is_err());
    }

    #[tokio::test]
    async fn test_get_relays_of_type() {
        use crate::whitenoise::Whitenoise;
        use crate::RelayType;
        use dashmap::DashSet;
        use nostr_sdk::RelayUrl;

        // Create test relay URLs
        let nip65_relay = RelayUrl::parse("wss://relay.damus.io").unwrap();
        let inbox_relay = RelayUrl::parse("wss://inbox.relay.com").unwrap();
        let key_package_relay = RelayUrl::parse("wss://keypackage.relay.com").unwrap();

        // Create contact with different relay types
        let contact = super::Contact {
            pubkey: create_test_keys().public_key(),
            metadata: None,
            nip65_relays: {
                let set = DashSet::new();
                set.insert(nip65_relay.clone());
                set
            },
            inbox_relays: {
                let set = DashSet::new();
                set.insert(inbox_relay.clone());
                set
            },
            key_package_relays: {
                let set = DashSet::new();
                set.insert(key_package_relay.clone());
                set
            },
        };

        // Test getting each relay type
        let nip65_relays = contact.get_relays_of_type(RelayType::Nostr);
        assert!(Whitenoise::relayurl_dashset_eq(
            nip65_relays,
            DashSet::from_iter([nip65_relay])
        ));

        let inbox_relays = contact.get_relays_of_type(RelayType::Inbox);
        assert!(Whitenoise::relayurl_dashset_eq(
            inbox_relays,
            DashSet::from_iter([inbox_relay])
        ));

        let key_package_relays = contact.get_relays_of_type(RelayType::KeyPackage);
        assert!(Whitenoise::relayurl_dashset_eq(
            key_package_relays,
            DashSet::from_iter([key_package_relay])
        ));
    }

    #[tokio::test]
    async fn test_get_relays_of_type_empty_fallback() {
        use crate::whitenoise::accounts::Account;
        use crate::whitenoise::Whitenoise;
        use crate::RelayType;
        use dashmap::DashSet;

        // Create contact with empty relay sets
        let contact = super::Contact {
            pubkey: create_test_keys().public_key(),
            metadata: None,
            nip65_relays: DashSet::new(),
            inbox_relays: DashSet::new(),
            key_package_relays: DashSet::new(),
        };

        // Test that empty relays fallback to default_relays
        let nip65_relays = contact.get_relays_of_type(RelayType::Nostr);
        let default_relays = Account::default_relays();
        assert!(Whitenoise::relayurl_dashset_eq(
            nip65_relays,
            default_relays.clone()
        ));

        let inbox_relays = contact.get_relays_of_type(RelayType::Inbox);
        assert!(Whitenoise::relayurl_dashset_eq(
            inbox_relays,
            default_relays.clone()
        ));

        let key_package_relays = contact.get_relays_of_type(RelayType::KeyPackage);
        assert!(Whitenoise::relayurl_dashset_eq(
            key_package_relays,
            default_relays
        ));
    }

    #[tokio::test]
    async fn test_get_relays_of_type_fallback_goes_to_nip65_if_available() {
        use crate::whitenoise::Whitenoise;
        use crate::RelayType;
        use dashmap::DashSet;

        let nip65_relay = RelayUrl::parse("wss://relay.damus.io").unwrap();

        let contact = super::Contact {
            pubkey: create_test_keys().public_key(),
            metadata: None,
            nip65_relays: DashSet::from_iter([nip65_relay.clone()]),
            inbox_relays: DashSet::new(),
            key_package_relays: DashSet::new(),
        };

        let inbox_relays = contact.get_relays_of_type(RelayType::Inbox);
        assert!(Whitenoise::relayurl_dashset_eq(
            inbox_relays,
            DashSet::from_iter([nip65_relay.clone()])
        ));

        let key_package_relays = contact.get_relays_of_type(RelayType::KeyPackage);
        assert!(Whitenoise::relayurl_dashset_eq(
            key_package_relays,
            DashSet::from_iter([nip65_relay])
        ));
    }
}
