//! Nostr API for Whitenoise
//! This module provides methods for interacting with the Nostr network, including
//! loading metadata, relays, and contact lists for users.
//!
//! All methods in this module only load data from the nostr cache. They do not perform any network requests.
//! The cache is updated by the nostr manager in the backaground.

use crate::error::Result;
use crate::relays::RelayType;
use crate::{OnboardingState, Whitenoise};
use nostr_sdk::prelude::*;
use std::collections::HashMap;

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
    /// Returns a [`WhitenoiseError`] if the metadata query fails.
    pub async fn load_metadata(&self, pubkey: PublicKey) -> Result<Option<Metadata>> {
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
    /// Returns a [`WhitenoiseError`] if the relay query fails.
    pub async fn load_relays(
        &self,
        pubkey: PublicKey,
        relay_type: RelayType,
    ) -> Result<Vec<RelayUrl>> {
        let relays = self.nostr.query_user_relays(pubkey, relay_type).await?;
        Ok(relays)
    }

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
    /// Returns a [`WhitenoiseError`] if the contact list query fails.
    pub async fn load_contact_list(
        &self,
        pubkey: PublicKey,
    ) -> Result<HashMap<PublicKey, Option<Metadata>>> {
        let contacts = self.nostr.query_user_contact_list(pubkey).await?;
        Ok(contacts)
    }

    pub async fn load_key_package(&self, pubkey: PublicKey) -> Result<Option<Event>> {
        let key_package = self.nostr.query_user_key_package(pubkey).await?;
        Ok(key_package)
    }

    pub async fn load_onboarding_state(&self, pubkey: PublicKey) -> Result<OnboardingState> {
        let mut onboarding_state = OnboardingState::default();

        let inbox_relays = self.load_relays(pubkey, RelayType::Inbox).await?;
        let key_package_relays = self.load_relays(pubkey, RelayType::KeyPackage).await?;
        let key_package_published = self.load_key_package(pubkey).await?;

        onboarding_state.inbox_relays = !inbox_relays.is_empty();
        onboarding_state.key_package_relays = !key_package_relays.is_empty();
        onboarding_state.key_package_published = key_package_published.is_some();

        Ok(onboarding_state)
    }
}
