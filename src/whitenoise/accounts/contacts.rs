use std::collections::HashMap;

use nostr::key::PublicKey;

use crate::whitenoise::accounts::{Account, OnboardingState};
use crate::whitenoise::error::{Result, WhitenoiseError};
use crate::whitenoise::relays::RelayType;
use crate::whitenoise::Whitenoise;

use nostr_sdk::prelude::*;

impl Whitenoise {
    // ============================================================================
    // CONTACT MANAGEMENT
    // ============================================================================

    /// Loads a user's contact list from the Nostr network.
    ///
    /// This method retrieves the user's contact list, which contains the public keys
    /// of other users they follow. For each contact, it also includes their metadata
    /// if available.
    ///
    /// # Arguments
    ///
    /// * `pubkey` - The `PublicKey` of the user whose contact list should be fetched.
    ///
    /// # Returns
    ///
    /// Returns `Ok(HashMap<PublicKey, Option<Metadata>>)` where the keys are the public keys
    /// of contacts and the values are their associated metadata (if available).
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if the contact list query fails.
    pub async fn fetch_contacts(
        &self,
        pubkey: PublicKey,
    ) -> Result<HashMap<PublicKey, Option<Metadata>>> {
        if !self.logged_in(&pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        let contacts = self.nostr.query_user_contact_list(pubkey).await?;
        Ok(contacts)
    }

    pub async fn fetch_key_package_event(&self, pubkey: PublicKey) -> Result<Option<Event>> {
        let key_package = self.nostr.query_user_key_package(pubkey).await?;
        Ok(key_package)
    }

    pub async fn fetch_onboarding_state(&self, pubkey: PublicKey) -> Result<OnboardingState> {
        let mut onboarding_state = OnboardingState::default();

        let inbox_relays = self.fetch_relays(pubkey, RelayType::Inbox).await?;
        let key_package_relays = self.fetch_relays(pubkey, RelayType::KeyPackage).await?;
        let key_package_published = self.fetch_key_package_event(pubkey).await?;

        onboarding_state.inbox_relays = !inbox_relays.is_empty();
        onboarding_state.key_package_relays = !key_package_relays.is_empty();
        onboarding_state.key_package_published = key_package_published.is_some();

        Ok(onboarding_state)
    }

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
        let current_contacts = self.fetch_contacts(account.pubkey).await?;

        // Check if contact already exists
        if current_contacts.contains_key(&contact_pubkey) {
            return Err(WhitenoiseError::ContactList(format!(
                "Contact {} already exists in contact list",
                contact_pubkey.to_hex()
            )));
        }

        // Create new contact list with the added contact
        let mut new_contacts: Vec<PublicKey> = current_contacts.keys().cloned().collect();
        new_contacts.push(contact_pubkey);

        // Publish the updated contact list
        self.publish_contact_list(account, new_contacts).await?;

        tracing::info!(
            target: "whitenoise::add_contact",
            "Added contact {} to account {}",
            contact_pubkey.to_hex(),
            account.pubkey.to_hex()
        );

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
        let current_contacts = self.fetch_contacts(account.pubkey).await?;

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

        // Get relays with fallback to defaults if user hasn't configured any
        let relays_to_use = self
            .fetch_relays_with_fallback(account.pubkey, RelayType::Nostr)
            .await?;

        // Publish the event
        let result = self
            .nostr
            .publish_event_builder_with_signer(event, &relays_to_use, keys.clone())
            .await?;

        // Update subscription for contact list metadata - use same relay logic
        self.nostr
            .update_contacts_metadata_subscription_with_signer(account.pubkey, relays_to_use, keys)
            .await?;

        tracing::debug!(
            target: "whitenoise::publish_contact_list",
            "Published contact list event: {:?}",
            result
        );

        Ok(())
    }
}
