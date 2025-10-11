use std::{collections::HashSet, str::FromStr};

use chrono::{DateTime, Utc};
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

use crate::whitenoise::{Whitenoise, accounts::Account, error::Result};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Hash)]
pub struct Relay {
    pub id: Option<i64>,
    pub url: RelayUrl,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum RelayType {
    Nip65,
    Inbox,
    KeyPackage,
}

impl FromStr for RelayType {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "nip65" => Ok(Self::Nip65),
            "inbox" => Ok(Self::Inbox),
            "key_package" => Ok(Self::KeyPackage),
            _ => Err(format!("Invalid relay type: {}", s)),
        }
    }
}

impl From<RelayType> for u16 {
    fn from(relay_type: RelayType) -> Self {
        match relay_type {
            RelayType::Nip65 => 10002,
            RelayType::Inbox => 10050,
            RelayType::KeyPackage => 10051,
        }
    }
}

impl From<RelayType> for String {
    fn from(relay_type: RelayType) -> Self {
        match relay_type {
            RelayType::Nip65 => "nip65".to_string(),
            RelayType::Inbox => "inbox".to_string(),
            RelayType::KeyPackage => "key_package".to_string(),
        }
    }
}

impl From<RelayType> for Kind {
    fn from(relay_type: RelayType) -> Self {
        match relay_type {
            RelayType::Nip65 => Kind::RelayList,
            RelayType::Inbox => Kind::InboxRelays,
            RelayType::KeyPackage => Kind::MlsKeyPackageRelays,
        }
    }
}

impl From<Kind> for RelayType {
    fn from(kind: Kind) -> Self {
        match kind {
            Kind::RelayList => RelayType::Nip65,
            Kind::InboxRelays => RelayType::Inbox,
            Kind::MlsKeyPackageRelays => RelayType::KeyPackage,
            _ => RelayType::Nip65, // Default fallback
        }
    }
}

impl Relay {
    pub(crate) fn new(url: &RelayUrl) -> Self {
        Relay {
            id: None,
            url: url.clone(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    pub(crate) fn defaults() -> Vec<Relay> {
        let urls: &[&str] = if cfg!(debug_assertions) {
            &["ws://localhost:8080", "ws://localhost:7777"]
        } else {
            &[
                "wss://relay.damus.io",
                "wss://relay.primal.net",
                "wss://nos.lol",
            ]
        };

        urls.iter()
            .filter_map(|&url_str| RelayUrl::parse(url_str).ok())
            .map(|url| Relay::new(&url))
            .collect()
    }

    pub(crate) fn urls<'a, I>(relays: I) -> Vec<RelayUrl>
    where
        I: IntoIterator<Item = &'a Relay>,
    {
        relays.into_iter().map(|r| r.url.clone()).collect()
    }
}

impl Whitenoise {
    pub async fn find_or_create_relay_by_url(&self, url: &RelayUrl) -> Result<Relay> {
        Relay::find_or_create_by_url(url, &self.database).await
    }

    /// Get connection status for all of an account's relays.
    ///
    /// This method returns a list of relay statuses for relays that are configured
    /// for the given account. It retrieves relay URLs from the account's relay lists
    /// (NIP-65, inbox, and key package relays) and returns the current connection
    /// status from the Nostr client.
    ///
    /// # Arguments
    ///
    /// * `account` - The account whose relay statuses should be retrieved.
    ///
    /// # Returns
    ///
    /// Returns a vector of tuples containing relay URLs and their connection status.
    pub async fn get_account_relay_statuses(
        &self,
        account: &Account,
    ) -> Result<Vec<(RelayUrl, RelayStatus)>> {
        // Get all relay URLs for this user across all types
        let mut all_relays = Vec::new();
        all_relays.extend(account.nip65_relays(self).await?);
        all_relays.extend(account.inbox_relays(self).await?);
        all_relays.extend(account.key_package_relays(self).await?);

        // Remove duplicates by collecting unique relay URLs
        let mut unique_relay_urls = HashSet::new();
        for relay in all_relays {
            unique_relay_urls.insert(relay.url);
        }

        // Get current relay statuses from the Nostr client
        let mut relay_statuses = Vec::new();

        for relay_url in unique_relay_urls {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_relay(url: &RelayUrl) -> super::Relay {
        super::Relay {
            id: None,
            url: url.clone(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn test_urls_empty_list() {
        let relays: Vec<super::Relay> = vec![];
        let urls = super::Relay::urls(&relays);
        assert_eq!(urls.len(), 0);
    }

    #[test]
    fn test_urls_extracts_and_preserves_order() {
        let url1 = RelayUrl::parse("wss://relay1.example.com").unwrap();
        let url2 = RelayUrl::parse("wss://relay2.example.com").unwrap();
        let url3 = RelayUrl::parse("wss://relay3.example.com").unwrap();

        let relays = vec![
            create_test_relay(&url1),
            create_test_relay(&url2),
            create_test_relay(&url3),
        ];

        let urls = super::Relay::urls(&relays);

        assert_eq!(urls, vec![url1, url2, url3]);
    }
}
