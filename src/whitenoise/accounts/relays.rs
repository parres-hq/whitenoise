use crate::whitenoise::error::{Result, WhitenoiseError};
use crate::whitenoise::Account;
use crate::whitenoise::Whitenoise;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

/// A row in the relays table
#[derive(Serialize, Deserialize, Debug, Clone, sqlx::FromRow)]
pub struct RelayRow {
    pub url: String,
    pub relay_type: String,
    pub account_pubkey: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Relay {
    pub url: String,
    pub relay_type: RelayType,
    pub account_pubkey: PublicKey,
}

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
    /// Loads the Nostr metadata for a contact by their public key.
    ///
    /// This method queries the Nostr network for user metadata associated with the provided public key.
    /// The metadata includes information such as display name, profile picture, and other user details
    /// that have been published to the Nostr network.
    ///
    /// # Arguments
    ///
    /// * `pubkey` - The `PublicKey` of the contact whose metadata should be fetched.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some(Metadata))` if metadata is found, `Ok(None)` if no metadata is available,
    /// or an error if the query fails.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if the metadata query fails.
    pub async fn fetch_metadata(&self, pubkey: PublicKey) -> Result<Option<Metadata>> {
        if !self.logged_in(&pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        let metadata = self.nostr.query_user_metadata(pubkey).await?;
        Ok(metadata)
    }

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
    /// Returns `Ok(Vec<RelayUrl>)` containing the list of relay URLs, or an error if the query fails.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if the relay query fails.
    pub async fn fetch_relays(
        &self,
        pubkey: PublicKey,
        relay_type: RelayType,
    ) -> Result<Vec<RelayUrl>> {
        let relays = self.nostr.query_user_relays(pubkey, relay_type).await?;
        Ok(relays)
    }

    /// Fetches user relays for the specified type, falling back to default client relays if empty.
    ///
    /// This helper method abstracts the common pattern of checking if user-specific relays
    /// are configured and falling back to default client relays when they're not available.
    /// This is particularly useful during onboarding and in test environments where users
    /// haven't configured relays yet.
    ///
    /// # Arguments
    ///
    /// * `pubkey` - The `PublicKey` of the user whose relays should be fetched.
    /// * `relay_type` - The type of relays to fetch (Nostr, Inbox, or KeyPackage).
    ///
    /// # Returns
    ///
    /// Returns `Ok(Vec<RelayUrl>)` containing user relays if available, otherwise default client relays.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if either the relay query or default relay fetch fails.
    pub(crate) async fn fetch_relays_with_fallback(
        &self,
        pubkey: PublicKey,
        relay_type: RelayType,
    ) -> Result<Vec<RelayUrl>> {
        let user_relays = self.fetch_relays(pubkey, relay_type).await?;

        if user_relays.is_empty() {
            self.nostr.relays().await.map_err(WhitenoiseError::from)
        } else {
            Ok(user_relays)
        }
    }

    /// Updates the metadata for the given account by publishing a new metadata event to Nostr.
    ///
    /// This method takes the provided metadata, creates a Nostr metadata event (Kind::Metadata),
    /// and publishes it to the account's relays. It also updates the account's `last_synced` timestamp
    /// in the database to reflect the successful publication.
    ///
    /// # Arguments
    ///
    /// * `metadata` - The new `Metadata` to publish for the account.
    /// * `account` - A reference to the `Account` whose metadata should be updated.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful publication and database update.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if:
    /// * The metadata cannot be serialized to JSON
    /// * The account's private key cannot be retrieved from the secret store
    /// * The event publication fails
    /// * The database update fails
    pub async fn update_metadata(&self, metadata: &Metadata, account: &Account) -> Result<()> {
        if !self.logged_in(&account.pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        tracing::debug!(
            target: "whitenoise::update_metadata",
            "Updating metadata for account: {}",
            account.pubkey.to_hex()
        );

        // Serialize metadata to JSON
        let metadata_json = serde_json::to_string(metadata)?;

        // Create metadata event
        let event = EventBuilder::new(Kind::Metadata, metadata_json);

        // Get signing keys for the account
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
            .publish_event_builder_with_signer(event, &relays_to_use, keys)
            .await?;

        tracing::debug!(
            target: "whitenoise::update_metadata",
            "Published metadata event: {:?}",
            result
        );

        Ok(())
    }

    /// Updates the relay list for the given account by publishing a new relay list event to Nostr.
    ///
    /// This method takes the provided relay URLs and relay type, creates the appropriate relay list event
    /// (Nostr relays, Inbox relays, or Key Package relays), and publishes it to the account's relays.
    /// The relay list event contains the provided relay URLs as relay tags.
    ///
    /// # Arguments
    ///
    /// * `account` - A reference to the `Account` whose relay list should be updated.
    /// * `relay_type` - The type of relay list to update (Nostr, Inbox, or KeyPackage).
    /// * `relays` - A vector of `RelayUrl` specifying the relays to include in the event.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful publication.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if:
    /// * The account's private key cannot be retrieved from the secret store
    /// * The event creation fails
    /// * The event publication fails
    pub async fn update_relays(
        &self,
        account: &Account,
        relay_type: RelayType,
        relays: Vec<RelayUrl>,
    ) -> Result<()> {
        if !self.logged_in(&account.pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        tracing::debug!(
            target: "whitenoise::update_account_relays",
            "Updating {:?} relays for account: {} with {} relays",
            relay_type,
            account.pubkey.to_hex(),
            relays.len()
        );

        // Use the existing helper method to publish the relay list
        self.publish_relay_list_for_account(account, relays, relay_type)
            .await?;

        tracing::debug!(
            target: "whitenoise::update_account_relays",
            "Successfully updated {:?} relays for account: {}",
            relay_type,
            account.pubkey.to_hex()
        );

        Ok(())
    }
}
