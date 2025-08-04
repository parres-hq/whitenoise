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

    /// Add relay to the nip65 relays list and establishes the connection
    /// Update the relay list in the database
    pub async fn add_nip65_relay(&self, account: &Account, relay: RelayUrl) -> Result<()> {
        self.nostr.connect_to_relay(relay.clone()).await?;
        account.nip65_relays.insert(relay.clone());
        
        // It is possible that the user have a relay list published in this added relay
        // To avoid over-writing it, we check if that event is published
        let list = DashSet::new();
        list.insert(relay);
        let existing_relay_list = self.fetch_relays_from(list, account.pubkey, RelayType::Nostr).await?;
        for relay in existing_relay_list {
            account.nip65_relays.insert(relay);
        }

        self.update_account_relays_db(
            &account.pubkey,
            account.nip65_relays.clone().into_iter().collect(),
            RelayType::Nostr,
        )
        .await?;
        
        self.publish_relay_list_for_account(account, RelayType::Nostr).await
    }

    /// Add relay to the inbox relays list and establishes the connection
    /// Update the relay list in the database
    pub async fn add_inbox_relay(&self, account: &Account, relay: RelayUrl) -> Result<()> {
        self.nostr.connect_to_relay(relay.clone()).await?;
        account.inbox_relays.insert(relay);
        self.update_account_relays_db(
            &account.pubkey,
            account.inbox_relays.clone().into_iter().collect(),
            RelayType::Inbox,
        )
        .await?;
        self.publish_relay_list_for_account(account, RelayType::Inbox)
            .await
    }

    /// Add relay to the inbox relays list, does not establish the connection
    /// Update the relay list in the database
    pub async fn add_key_package_relay(&self, account: &Account, relay: RelayUrl) -> Result<()> {
        account.key_package_relays.insert(relay);
        self.update_account_relays_db(
            &account.pubkey,
            account.key_package_relays.clone().into_iter().collect(),
            RelayType::KeyPackage,
        )
        .await?;
        self.publish_relay_list_for_account(account, RelayType::KeyPackage)
            .await
    }

    pub async fn remove_nip65_relay(&self, account: &Account, relay: RelayUrl) -> Result<()> {
        let original_list = account.nip65_relays.clone();
        account.nip65_relays.remove(&relay);

        // Publish updated relay list to original nip65 relays
        let tags: Vec<Tag> = account.nip65_relays
            .clone()
            .into_iter()
            .map(|url| Tag::custom(TagKind::Relay, [url.to_string()]))
            .collect();
        tracing::debug!("Publishing relay list tags {:?}", tags);

        let event = EventBuilder::new(RelayType::Nostr.into(), "").tags(tags);
        let keys = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?;

        let result = self
            .nostr
            .publish_event_builder_with_signer(event.clone(), original_list, keys)
            .await?;
        tracing::debug!(target: "whitenoise::publish_relay_list", "Published relay list event to Nostr: {:?}", result);

        self.update_account_relays_db(
            &account.pubkey,
            account.nip65_relays.clone().into_iter().collect(),
            RelayType::Nostr,
        )
        .await?;
        self.nostr.disconnect_from_relay(relay).await.map_err(WhitenoiseError::from)
    }

    pub async fn remove_inbox_relay(&self, account: &Account, relay: RelayUrl) -> Result<()> {
        account.inbox_relays.remove(&relay);
        self.update_account_relays_db(
            &account.pubkey,
            account.inbox_relays.clone().into_iter().collect(),
            RelayType::Inbox,
        )
        .await?;
        self.publish_relay_list_for_account(account, RelayType::Inbox)
            .await?;
        self.nostr.disconnect_from_relay(relay).await.map_err(WhitenoiseError::from)
    }

    pub async fn remove_key_package_relay(&self, account: &Account, relay: RelayUrl) -> Result<()> {
        account.key_package_relays.remove(&relay);
        self.update_account_relays_db(
            &account.pubkey,
            account.key_package_relays.clone().into_iter().collect(),
            RelayType::KeyPackage,
        )
        .await?;
        self.publish_relay_list_for_account(account, RelayType::KeyPackage)
            .await
    }

    pub(crate) async fn publish_account_relay_info(&self, account: &Account) -> Result<()> {
        if !self.logged_in(&account.pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        self.publish_relay_list_for_account(account, RelayType::Nostr)
            .await?;
        self.publish_relay_list_for_account(account, RelayType::Inbox)
            .await?;
        self.publish_relay_list_for_account(account, RelayType::KeyPackage)
            .await
    }

    pub(crate) async fn publish_relay_list_for_account(
        &self,
        account: &Account,
        relay_type: RelayType,
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

        let relays_to_use = account.nip65_relays.clone();
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
            .publish_event_builder_with_signer(event.clone(), relays_to_use, keys)
            .await?;
        tracing::debug!(target: "whitenoise::publish_relay_list", "Published relay list event to Nostr: {:?}", result);

        Ok(())
    }

    pub(crate) async fn update_account_relays_db(
        &self,
        pubkey: &PublicKey,
        relays: Vec<RelayUrl>,
        relay_type: RelayType,
    ) -> Result<()> {
        // 1. Ensure the account exists
        if !self.logged_in(pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        // 2. Serialize the Vec<RelayUrl> into a JSON string
        let relays_json = serde_json::to_string(&relays)?;

        // 3. Pick the right column name
        let column = match relay_type {
            RelayType::Nostr => "nip65_relays",
            RelayType::Inbox => "inbox_relays",
            RelayType::KeyPackage => "key_package_relays",
        };

        // 4. Build & execute the UPDATE
        let sql = format!("UPDATE accounts SET {} = ? WHERE pubkey = ?", column);
        let result = sqlx::query(&sql)
            .bind(relays_json)
            .bind(pubkey.to_hex())
            .execute(&self.database.pool)
            .await?;

        // 5. Make sure something was updated
        if result.rows_affected() < 1 {
            Err(WhitenoiseError::AccountNotFound)
        } else {
            Ok(())
        }
    }

    #[cfg(test)]
    /// Fetches the list of relays for `pubkey` and the given `relay_type`.
    async fn get_account_relays_db(
        &self,
        pubkey: &PublicKey,
        relay_type: RelayType,
    ) -> Result<Vec<RelayUrl>> {
        // 1. Ensure the account exists
        if !self.logged_in(pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        // 2. Pick the right column name
        let column = match relay_type {
            RelayType::Nostr       => "nip65_relays",
            RelayType::Inbox       => "inbox_relays",
            RelayType::KeyPackage  => "key_package_relays",
        };

        // 3. Build & execute the SELECT
        let sql = format!("SELECT {} FROM accounts WHERE pubkey = ?", column);
        let relays_json: String = sqlx::query_scalar(&sql)
            .bind(pubkey.to_hex())
            .fetch_one(&self.database.pool)
            .await?;

        // 4. Deserialize JSON into Vec<RelayUrl>
        let relays: Vec<RelayUrl> = serde_json::from_str(&relays_json)?;

        Ok(relays)
    }
}

#[cfg(test)]
mod tests {
    use nostr::types::RelayUrl;

    use crate::whitenoise::test_utils::test_get_whitenoise;
    use crate::{RelayType, Whitenoise};

    
    #[tokio::test]
    async fn test_add_remove_relay() {
        let l7777 = RelayUrl::parse("ws://localhost:8080").unwrap();
        let whitenoise = test_get_whitenoise().await;
        let account = whitenoise.create_identity().await.unwrap();

        // nip65 relays remove
        whitenoise.remove_nip65_relay(&account, l7777.clone()).await.unwrap();
        assert_eq!(account.nip65_relays.len(), 1);
        let relay_list_db = whitenoise.get_account_relays_db(&account.pubkey, RelayType::Nostr).await.unwrap();
        assert!(Whitenoise::relayurl_dashset_eq(relay_list_db.into_iter().collect(), account.nip65_relays.clone()));
        let relay_list = whitenoise.fetch_relays_from(account.nip65_relays.clone(), account.pubkey, RelayType::Nostr).await.unwrap();
        assert!(Whitenoise::relayurl_dashset_eq(relay_list.clone(), account.nip65_relays.clone()), "{relay_list:?}");

        // nip65 relays add
        whitenoise.add_nip65_relay(&account, l7777.clone()).await.unwrap();
        let relay_list_db = whitenoise.get_account_relays_db(&account.pubkey, RelayType::Nostr).await.unwrap();
        assert!(Whitenoise::relayurl_dashset_eq(relay_list_db.into_iter().collect(), account.nip65_relays.clone()));
        let relay_list = whitenoise.fetch_relays_from(account.nip65_relays.clone(), account.pubkey, RelayType::Nostr).await.unwrap();
        assert!(Whitenoise::relayurl_dashset_eq(relay_list, account.nip65_relays.clone()));
        assert_eq!(account.nip65_relays.len(), 2);
    }
}
