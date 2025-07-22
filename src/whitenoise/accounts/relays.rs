use crate::whitenoise::accounts::Account;
use crate::whitenoise::error::Result;
use crate::whitenoise::Whitenoise;
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
    /// Returns `Ok(Vec<RelayUrl>)` containing the list of relay URLs, or an error if the query fails.
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if the relay query fails.
    pub async fn fetch_relays_from(
        &self,
        discovery_relays: Vec<RelayUrl>,
        pubkey: PublicKey,
        relay_type: RelayType,
    ) -> Result<Vec<RelayUrl>> {
        let relays = self
            .nostr
            .fetch_user_relays(pubkey, relay_type, discovery_relays)
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
        all_relays.extend(account.discovery_relays.clone());
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
}
