use crate::types::ImageType;
use crate::whitenoise::accounts::Account;
use crate::whitenoise::accounts::AccountError;
use crate::whitenoise::error::Result;
use crate::whitenoise::relays::Relay;
use crate::whitenoise::users::User;
use crate::whitenoise::{Whitenoise, WhitenoiseError};
use crate::RelayType;
use chrono::Utc;
use nostr_blossom::client::BlossomClient;
use nostr_mls::prelude::*;
use nostr_mls_sqlite_storage::NostrMlsSqliteStorage;
use std::collections::HashSet;
use std::path::Path;

impl Account {
    pub(crate) async fn new(
        whitenoise: &Whitenoise,
        keys: Option<Keys>,
    ) -> Result<(Account, Keys)> {
        let keys = keys.unwrap_or_else(Keys::generate);

        let (user, _created) =
            User::find_or_create_by_pubkey(&keys.public_key(), &whitenoise.database).await?;

        let account = Account {
            id: None,
            user_id: user.id.unwrap(),
            pubkey: keys.public_key(),
            last_synced_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        Ok((account, keys))
    }

    /// Retrieves the account's configured relays for a specific relay type.
    ///
    /// This method fetches the locally cached relays associated with this account
    /// for the specified relay type. Different relay types serve different purposes
    /// in the Nostr ecosystem and are published as separate relay list events.
    ///
    /// # Arguments
    ///
    /// * `relay_type` - The type of relays to retrieve:
    ///   - `RelayType::Nip65` - General purpose relays for reading/writing events (kind 10002)
    ///   - `RelayType::Inbox` - Specialized relays for receiving private messages (kind 10050)
    ///   - `RelayType::KeyPackage` - Relays that store MLS key packages (kind 10051)
    /// * `whitenoise` - The Whitenoise instance for database operations
    ///
    /// # Returns
    ///
    /// Returns a `Vec<Relay>` containing all relays configured for the account
    /// and the specified relay type. Returns an empty vector if no relays are
    /// configured for that type.
    ///
    /// # Errors
    ///
    /// This method will return an error if:
    /// - The account's user record cannot be found in the database
    /// - There's a database connection or query error during the lookup
    ///
    /// # Relay Types Explained
    ///
    /// ## NIP-65 Relays (General Purpose)
    /// - Used for publishing and reading most types of Nostr events
    /// - Includes metadata, notes, reactions, and other standard events
    /// - Published as kind 10002 relay list events
    /// - Typically the most comprehensive relay list
    ///
    /// ## Inbox Relays (Private Messages)
    /// - Specialized for receiving private messages and gift wrap events
    /// - Other users send private messages to these relays
    /// - Published as kind 10050 relay list events
    /// - Often a subset of NIP-65 relays optimized for message delivery
    ///
    /// ## Key Package Relays (MLS Groups)
    /// - Store MLS key packages needed for encrypted group messaging
    /// - Other users fetch key packages from these relays to add the account to groups
    /// - Published as kind 10051 relay list events
    /// - May be specialized relays that handle MLS-specific events
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use whitenoise::RelayType;
    ///
    /// // Get general purpose relays
    /// let nip65_relays = account.relays(RelayType::Nip65, &whitenoise).await?;
    /// println!("Account has {} general relays", nip65_relays.len());
    ///
    /// // Get inbox relays for private messaging
    /// let inbox_relays = account.relays(RelayType::Inbox, &whitenoise).await?;
    /// for relay in inbox_relays {
    ///     println!("Inbox relay: {}", relay.url);
    /// }
    ///
    /// // Get key package relays for MLS groups
    /// let kp_relays = account.relays(RelayType::KeyPackage, &whitenoise).await?;
    /// if kp_relays.is_empty() {
    ///     println!("No key package relays configured - account cannot join MLS groups");
    /// }
    /// ```
    pub async fn relays(
        &self,
        relay_type: RelayType,
        whitenoise: &Whitenoise,
    ) -> Result<Vec<Relay>> {
        let user = self.user(&whitenoise.database).await?;
        let relays = user.relays(relay_type, &whitenoise.database).await?;
        Ok(relays)
    }

    /// Helper method to retrieve the NIP-65 relays for this account.
    pub(crate) async fn nip65_relays(&self, whitenoise: &Whitenoise) -> Result<Vec<Relay>> {
        let user = self.user(&whitenoise.database).await?;
        let relays = user.relays(RelayType::Nip65, &whitenoise.database).await?;
        Ok(relays)
    }

    /// Helper method to retrieve the inbox relays for this account.
    pub(crate) async fn inbox_relays(&self, whitenoise: &Whitenoise) -> Result<Vec<Relay>> {
        let user = self.user(&whitenoise.database).await?;
        let relays = user.relays(RelayType::Inbox, &whitenoise.database).await?;
        Ok(relays)
    }

    /// Helper method to retrieve the key package relays for this account.
    pub(crate) async fn key_package_relays(&self, whitenoise: &Whitenoise) -> Result<Vec<Relay>> {
        let user = self.user(&whitenoise.database).await?;
        let relays = user
            .relays(RelayType::KeyPackage, &whitenoise.database)
            .await?;
        Ok(relays)
    }

    /// Adds a relay to the account's relay list for the specified relay type.
    ///
    /// This method adds a relay to the account's local relay configuration and automatically
    /// publishes the updated relay list to the Nostr network. The relay will be associated
    /// with the specified type (NIP-65, Inbox, or Key Package relays) and become part of
    /// the account's relay configuration for that purpose.
    ///
    /// # Arguments
    ///
    /// * `relay` - The relay to add to the account's relay list
    /// * `relay_type` - The type of relay list to add this relay to:
    ///   - `RelayType::Nip65` - General purpose relays (kind 10002)
    ///   - `RelayType::Inbox` - Inbox relays for private messages (kind 10050)
    ///   - `RelayType::KeyPackage` - Key package relays for MLS (kind 10051)
    /// * `whitenoise` - The Whitenoise instance for database and network operations
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the relay is successfully added to the local database and
    /// the updated relay list is published to the network.
    ///
    /// # Process Flow
    ///
    /// 1. **Local Update**: Adds the relay to the account's user record in the database
    /// 2. **Network Publishing**: Publishes the updated relay list to the account's relays
    /// 3. **Logging**: Records the successful addition for debugging
    ///
    /// # Errors
    ///
    /// This method will return an error if:
    /// - The account's user record cannot be found in the database
    /// - Database operations fail when adding the relay
    /// - Network publishing of the updated relay list fails
    /// - The relay URL is malformed or invalid
    ///
    /// # Relay Types
    ///
    /// Different relay types serve different purposes:
    /// - **NIP-65 relays**: General purpose relays for reading/writing most event types
    /// - **Inbox relays**: Specialized relays for receiving private messages and gift wraps
    /// - **Key package relays**: Relays that store MLS key packages for group messaging
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use nostr_sdk::RelayUrl;
    /// use whitenoise::{Relay, RelayType};
    ///
    /// // Add a general purpose relay
    /// let relay = Relay::new(RelayUrl::parse("wss://relay.example.com")?);
    /// account.add_relay(&relay, RelayType::Nip65, &whitenoise).await?;
    ///
    /// // Add an inbox relay for private messages
    /// let inbox_relay = Relay::new(RelayUrl::parse("wss://inbox.example.com")?);
    /// account.add_relay(&inbox_relay, RelayType::Inbox, &whitenoise).await?;
    /// ```
    ///
    /// # Network Effects
    ///
    /// This method triggers network activity:
    /// - Publishes the updated relay list event to the account's current relays
    /// - Other clients will see the updated relay configuration
    /// - The account will start using the new relay for the specified purpose
    pub async fn add_relay(
        &self,
        relay: &Relay,
        relay_type: RelayType,
        whitenoise: &Whitenoise,
    ) -> Result<()> {
        let user = self.user(&whitenoise.database).await?;
        user.add_relay(relay, relay_type, &whitenoise.database)
            .await?;
        whitenoise
            .background_publish_account_relay_list(self, relay_type)
            .await?;
        tracing::debug!(target: "whitenoise::accounts::add_relay", "Added relay to account: {:?}", relay.url);

        Ok(())
    }

    /// Removes a relay from the account's relay list for the specified relay type.
    ///
    /// This method removes a relay from the account's local relay configuration and automatically
    /// publishes the updated relay list to the Nostr network. The relay will be disassociated
    /// from the specified type and the account will stop using it for that purpose.
    ///
    /// # Arguments
    ///
    /// * `relay` - The relay to remove from the account's relay list
    /// * `relay_type` - The type of relay list to remove this relay from:
    ///   - `RelayType::Nip65` - General purpose relays (kind 10002)
    ///   - `RelayType::Inbox` - Inbox relays for private messages (kind 10050)
    ///   - `RelayType::KeyPackage` - Key package relays for MLS (kind 10051)
    /// * `whitenoise` - The Whitenoise instance for database and network operations
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the relay is successfully removed from the local database and
    /// the updated relay list is published to the network.
    ///
    /// # Process Flow
    ///
    /// 1. **Local Update**: Removes the relay from the account's user record in the database
    /// 2. **Network Publishing**: Publishes the updated relay list to the account's remaining relays
    /// 3. **Logging**: Records the successful removal for debugging
    ///
    /// # Errors
    ///
    /// This method will return an error if:
    /// - The account's user record cannot be found in the database
    /// - Database operations fail when removing the relay
    /// - Network publishing of the updated relay list fails
    /// - The relay is not found in the account's relay list for the specified type
    ///
    /// # Safety Considerations
    ///
    /// - **Last Relay Warning**: Be cautious when removing the last relay of a type, as this
    ///   may leave the account unable to perform certain operations
    /// - **Network Isolation**: Removing all NIP-65 relays may isolate the account from the network
    /// - **Message Delivery**: Removing inbox relays may prevent receipt of private messages
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use nostr_sdk::RelayUrl;
    /// use whitenoise::{Relay, RelayType};
    ///
    /// // Remove a general purpose relay
    /// let relay = Relay::new(RelayUrl::parse("wss://old-relay.example.com")?);
    /// account.remove_relay(&relay, RelayType::Nip65, &whitenoise).await?;
    ///
    /// // Remove an inbox relay
    /// let inbox_relay = Relay::new(RelayUrl::parse("wss://old-inbox.example.com")?);
    /// account.remove_relay(&inbox_relay, RelayType::Inbox, &whitenoise).await?;
    /// ```
    ///
    /// # Network Effects
    ///
    /// This method triggers network activity:
    /// - Publishes the updated relay list event to the account's remaining relays
    /// - Other clients will see the updated relay configuration
    /// - The account will stop using the removed relay for the specified purpose
    /// - May disconnect from the relay if it's not used for other purposes
    pub async fn remove_relay(
        &self,
        relay: &Relay,
        relay_type: RelayType,
        whitenoise: &Whitenoise,
    ) -> Result<()> {
        let user = self.user(&whitenoise.database).await?;
        user.remove_relay(relay, relay_type, &whitenoise.database)
            .await?;
        whitenoise
            .background_publish_account_relay_list(self, relay_type)
            .await?;
        tracing::debug!(target: "whitenoise::accounts::remove_relay", "Removed relay from account: {:?}", relay.url);
        Ok(())
    }

    /// Retrieves the cached metadata for this account.
    ///
    /// This method returns the account's stored metadata from the local database without
    /// performing any network requests. The metadata contains profile information such as
    /// display name, about text, picture URL, and other profile fields as defined by NIP-01.
    ///
    /// # Arguments
    ///
    /// * `whitenoise` - The Whitenoise instance used to access the database
    ///
    /// # Returns
    ///
    /// Returns the account's `Metadata` object containing profile information.
    /// The metadata is cloned from the cached record, so modifications to the
    /// returned value will not affect the stored data.
    ///
    /// # Errors
    ///
    /// This method will return an error if:
    /// - The account's user record cannot be found in the database
    /// - There's a database connection or query error during the lookup
    ///
    /// # Metadata Fields
    ///
    /// The returned metadata may contain any of these standard NIP-01 fields:
    /// - `name` - The account holder's name/username
    /// - `display_name` - The account holder's display name
    /// - `about` - Bio/description text
    /// - `picture` - URL to profile picture
    /// - `banner` - URL to banner image
    /// - `website` - Personal website URL
    /// - `nip05` - NIP-05 identifier for verification
    /// - And other custom fields that may have been set
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// // Get account metadata
    /// let metadata = account.metadata(&whitenoise).await?;
    ///
    /// // Access specific fields
    /// if let Some(name) = &metadata.name {
    ///     println!("Account name: {}", name);
    /// }
    ///
    /// if let Some(about) = &metadata.about {
    ///     println!("About: {}", about);
    /// }
    ///
    /// if let Some(picture) = &metadata.picture {
    ///     println!("Profile picture: {}", picture);
    /// }
    /// ```
    ///
    /// # Performance Notes
    ///
    /// - This method only reads from the local database and is fast
    /// - No network requests are performed
    /// - The metadata is cached from previous sync operations
    /// - For up-to-date metadata, ensure recent synchronization has occurred
    pub async fn metadata(&self, whitenoise: &Whitenoise) -> Result<Metadata> {
        let user = self.user(&whitenoise.database).await?;
        Ok(user.metadata.clone())
    }

    /// Updates the account's metadata with new values and publishes to the network.
    ///
    /// This method updates the account's metadata in the local database with the provided
    /// values and automatically publishes a metadata event (kind 0) to the account's relays.
    /// This allows other users and clients to see the updated profile information.
    ///
    /// # Arguments
    ///
    /// * `metadata` - The new metadata to set for this account
    /// * `whitenoise` - The Whitenoise instance for database and network operations
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the metadata is successfully updated in the local database
    /// and published to the network.
    ///
    /// # Process Flow
    ///
    /// 1. **Validation**: Logs the update operation for debugging
    /// 2. **Local Update**: Updates the account's user record in the database
    /// 3. **Database Save**: Commits the metadata changes to the database
    /// 4. **Network Publishing**: Publishes the new metadata event to the account's relays
    ///
    /// # Errors
    ///
    /// This method will return an error if:
    /// - The account's user record cannot be found in the database
    /// - Database operations fail when saving the updated metadata
    /// - Network publishing of the metadata event fails
    /// - The account has no configured relays to publish to
    ///
    /// # Metadata Fields
    ///
    /// The metadata object can contain any of these standard NIP-01 fields:
    /// - `name` - Username or handle
    /// - `display_name` - Display name for UI
    /// - `about` - Bio or description text
    /// - `picture` - URL to profile picture
    /// - `banner` - URL to banner image
    /// - `website` - Personal website URL
    /// - `nip05` - NIP-05 identifier for verification
    /// - `lud06` or `lud16` - Lightning addresses for payments
    /// - Any custom fields specific to your application
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use nostr_sdk::Metadata;
    ///
    /// // Create new metadata
    /// let metadata = Metadata::new()
    ///     .name("alice")
    ///     .display_name("Alice Smith")
    ///     .about("Enthusiastic Nostr user")
    ///     .picture("https://example.com/alice.jpg")
    ///     .website("https://alice-blog.example.com");
    ///
    /// // Update account metadata
    /// account.update_metadata(&metadata, &whitenoise).await?;
    ///
    /// // The metadata is now updated locally and published to the network
    /// ```
    ///
    /// # Network Effects
    ///
    /// This method triggers network activity:
    /// - Publishes a kind 0 (metadata) event to the account's configured relays
    /// - Other clients subscribed to the account will receive the updated metadata
    /// - The changes become part of the account's public profile on Nostr
    /// - Relay operators may cache the new metadata for efficient retrieval
    pub async fn update_metadata(
        &self,
        metadata: &Metadata,
        whitenoise: &Whitenoise,
    ) -> Result<()> {
        tracing::debug!(target: "whitenoise::accounts::update_metadata", "Updating metadata for account: {:?}", self.pubkey);
        let mut user = self.user(&whitenoise.database).await?;
        user.metadata = metadata.clone();
        user.save(&whitenoise.database).await?;
        whitenoise.background_publish_account_metadata(self).await?;
        Ok(())
    }

    /// Uploads a profile picture to a Blossom server and updates the account's metadata.
    ///
    /// This method reads an image file from the specified path, uploads it to a Blossom server,
    /// and updates the account's metadata with the new profile picture URL. The image is
    /// uploaded using the Blossom API, and the resulting URL is stored in the account's metadata.
    ///
    /// # Arguments
    ///
    /// * `file_path` - The path to the image file to upload
    /// * `image_type` - The type of image being uploaded (e.g., JPEG, PNG)
    /// * `server` - The URL of the Blossom server to use for uploading
    /// * `whitenoise` - The Whitenoise instance for database and network operations
    ///
    /// # Returns
    ///
    /// Returns the URL of the uploaded profile picture as a `String`.
    ///
    /// # Errors
    ///
    /// This method will return an error if:
    /// - The image file cannot be read from the specified path
    /// - The image upload fails to the Blossom server
    /// - The account's Nostr keys cannot be retrieved from the secret store
    /// - There's a database connection or query error during the lookup
    pub async fn upload_profile_picture(
        &self,
        file_path: &str,
        image_type: ImageType,
        server: Url,
        whitenoise: &Whitenoise,
    ) -> Result<String> {
        let client = BlossomClient::new(server);
        let keys = whitenoise
            .secrets_store
            .get_nostr_keys_for_pubkey(&self.pubkey)?;
        let data = tokio::fs::read(file_path).await?;

        let descriptor = client
            .upload_blob(
                data,
                Some(image_type.mime_type().to_string()),
                None,
                Some(&keys),
            )
            .await
            .map_err(|err| WhitenoiseError::Other(anyhow::anyhow!(err)))?;

        Ok(descriptor.url.to_string())
    }

    pub(crate) fn create_nostr_mls(
        pubkey: PublicKey,
        data_dir: &Path,
    ) -> core::result::Result<NostrMls<NostrMlsSqliteStorage>, AccountError> {
        let mls_storage_dir = data_dir.join("mls").join(pubkey.to_hex());
        let storage = NostrMlsSqliteStorage::new(mls_storage_dir)?;
        Ok(NostrMls::new(storage))
    }

    pub(crate) fn load_nostr_group_ids(
        &self,
        whitenoise: &Whitenoise,
    ) -> core::result::Result<Vec<String>, AccountError> {
        let nostr_mls = Account::create_nostr_mls(self.pubkey, &whitenoise.config.data_dir)?;
        let groups = nostr_mls.get_groups()?;
        Ok(groups
            .iter()
            .map(|g| hex::encode(g.nostr_group_id))
            .collect())
    }

    pub(crate) async fn connect_relays(&self, whitenoise: &Whitenoise) -> Result<()> {
        let mut relays = HashSet::new();
        relays.extend(self.nip65_relays(whitenoise).await?);
        relays.extend(self.inbox_relays(whitenoise).await?);
        let urls: Vec<RelayUrl> = relays.iter().map(|r| r.url.clone()).collect();
        for url in urls {
            whitenoise.nostr.client.add_relay(url).await?;
        }
        whitenoise.nostr.client.connect().await;
        Ok(())
    }
}

impl Whitenoise {
    /// Creates a new identity (account) for the user.
    ///
    /// This method generates a new keypair, sets up the account with default relay lists,
    /// creates a metadata event with a generated petname, and fully configures the account
    /// for use in Whitenoise.
    ///
    /// # Returns
    ///
    /// Returns the newly created and fully configured `Account` on success.
    ///
    /// # Errors
    ///
    /// Returns a [`WhitenoiseError`] if any step fails. The operation is atomic with cleanup on failure.
    pub async fn create_identity(&self) -> Result<Account> {
        let keys = Keys::generate();
        tracing::debug!(target: "whitenoise::create_identity", "Generated new keypair: {}", keys.public_key().to_hex());

        let mut account = self.create_base_account_with_private_key(&keys).await?;
        tracing::debug!(target: "whitenoise::create_identity", "Keys stored in secret store and account saved to database");

        let (_user, _newly_created) = self.create_user_for_account(&account).await?;
        tracing::debug!(target: "whitenoise::create_identity", "User created for account: {:?}", account.pubkey);

        self.setup_relays_for_new_account(&mut account).await?;
        tracing::debug!(target: "whitenoise::create_identity", "Relays setup");
        tracing::debug!(target: "whitenoise::create_identity", "Nip65 relays: {:?}", account.nip65_relays(self).await?);
        tracing::debug!(target: "whitenoise::create_identity", "Inbox relays: {:?}", account.inbox_relays(self).await?);
        tracing::debug!(target: "whitenoise::create_identity", "Key package relays: {:?}", account.key_package_relays(self).await?);

        self.activate_account(&account).await?;
        tracing::debug!(target: "whitenoise::create_identity", "Account persisted and activated");

        self.setup_metadata(&account).await?;
        tracing::debug!(target: "whitenoise::create_identity", "Metadata setup");

        tracing::debug!(target: "whitenoise::create_identity", "Successfully created new identity: {}", account.pubkey.to_hex());
        Ok(account)
    }

    /// Logs in an existing user using a private key (nsec or hex format).
    ///
    /// This method parses the private key, checks if the account exists locally,
    /// and sets up the account for use. If the account doesn't exist locally,
    /// it treats it as an existing account and fetches data from the network.
    ///
    /// # Arguments
    ///
    /// * `nsec_or_hex_privkey` - The user's private key as a nsec string or hex-encoded string.
    ///
    /// # Returns
    ///
    /// Returns the fully configured `Account` associated with the provided private key on success.
    ///
    /// # Errors
    ///
    /// Returns a [`WhitenoiseError`] if the private key is invalid or account setup fails.
    pub async fn login(&self, nsec_or_hex_privkey: String) -> Result<Account> {
        let keys = Keys::parse(&nsec_or_hex_privkey)?;
        let pubkey = keys.public_key();
        tracing::debug!(target: "whitenoise::login", "Logging in with pubkey: {}", pubkey.to_hex());

        let mut account = self.create_base_account_with_private_key(&keys).await?;
        tracing::debug!(target: "whitenoise::login", "Keys stored in secret store and account saved to database");

        let (_user, _newly_created) = self.create_user_for_account(&account).await?;
        tracing::debug!(target: "whitenoise::login", "User created for account: {:?}", account.pubkey);

        // Always check for existing relay lists when logging in, even if the user is
        // newly created in our database, because the keypair might already exist in
        // the Nostr ecosystem with published relay lists from other apps
        self.setup_relays_for_existing_account(&mut account).await?;
        tracing::debug!(target: "whitenoise::login", "Relays setup");
        tracing::debug!(target: "whitenoise::login", "Nip65 relays: {:?}", account.nip65_relays(self).await?);
        tracing::debug!(target: "whitenoise::login", "Inbox relays: {:?}", account.inbox_relays(self).await?);
        tracing::debug!(target: "whitenoise::login", "Key package relays: {:?}", account.key_package_relays(self).await?);

        self.activate_account(&account).await?;
        tracing::debug!(target: "whitenoise::login", "Account persisted and activated");

        self.background_sync_account_data(&account).await?;
        tracing::debug!(target: "whitenoise::login", "Background data fetch triggered");

        tracing::debug!(target: "whitenoise::login", "Successfully logged in: {}", account.pubkey.to_hex());
        Ok(account)
    }

    /// Logs out the user associated with the given account.
    ///
    /// This method performs the following steps:
    /// - Removes the account from the database.
    /// - Removes the private key from the secret store.
    /// - Updates the active account if the logged-out account was active.
    /// - Removes the account from the in-memory accounts list.
    ///
    /// - NB: This method does not remove the MLS database for the account. If the user logs back in, the MLS database will be re-initialized and used again.
    ///
    /// # Arguments
    ///
    /// * `account` - The account to log out.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success.
    ///
    /// # Errors
    ///
    /// Returns a [`WhitenoiseError`] if there is a failure in removing the account or its private key.
    pub async fn logout(&self, pubkey: &PublicKey) -> Result<()> {
        let account = Account::find_by_pubkey(pubkey, &self.database).await?;
        // Delete the account from the database
        account.delete(&self.database).await?;

        // Remove the private key from the secret store
        self.secrets_store.remove_private_key_for_pubkey(pubkey)?;

        Ok(())
    }

    /// Returns the total number of accounts stored in the database.
    ///
    /// This method queries the database to count all accounts that have been created
    /// or imported into the Whitenoise instance. This includes both active accounts
    /// and any accounts that may have been created but are not currently in use.
    ///
    /// # Returns
    ///
    /// Returns the count of accounts as a `usize`. Returns 0 if no accounts exist.
    ///
    /// # Errors
    ///
    /// This method will return an error if:
    /// - There's a database connection or query error
    /// - The database is corrupted or inaccessible
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// // Check if any accounts exist
    /// let count = whitenoise.get_accounts_count().await?;
    /// if count == 0 {
    ///     println!("No accounts found, create a new one");
    /// } else {
    ///     println!("Found {} accounts", count);
    /// }
    /// ```
    pub async fn get_accounts_count(&self) -> Result<usize> {
        let accounts = Account::all(&self.database).await?;
        Ok(accounts.len())
    }

    /// Retrieves all accounts stored in the database.
    ///
    /// This method returns all accounts that have been created or imported into
    /// the Whitenoise instance. Each account represents a distinct identity with
    /// its own keypair, relay configurations, and associated data.
    ///
    /// # Returns
    ///
    /// Returns a `Vec<Account>` containing all accounts in the database.
    /// Returns an empty vector if no accounts exist.
    ///
    /// # Errors
    ///
    /// This method will return an error if:
    /// - There's a database connection or query error
    /// - Account data is corrupted or cannot be deserialized
    /// - The database is inaccessible
    ///
    /// # Account Information
    ///
    /// Each returned `Account` contains:
    /// - `id` - Database primary key (if saved)
    /// - `pubkey` - The account's Nostr public key
    /// - `user_id` - Reference to associated user record
    /// - `last_synced_at` - Timestamp of last data synchronization
    /// - `created_at` - Account creation timestamp
    /// - `updated_at` - Last update timestamp
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// // List all accounts
    /// let accounts = whitenoise.all_accounts().await?;
    /// for account in accounts {
    ///     println!("Account: {} (created: {})",
    ///         account.pubkey,
    ///         account.created_at
    ///     );
    /// }
    ///
    /// // Find the most recently created account
    /// let mut accounts = whitenoise.all_accounts().await?;
    /// accounts.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    /// if let Some(latest) = accounts.first() {
    ///     println!("Latest account: {}", latest.pubkey);
    /// }
    /// ```
    pub async fn all_accounts(&self) -> Result<Vec<Account>> {
        Account::all(&self.database).await
    }

    /// Finds and returns an account by its public key.
    ///
    /// This method searches the database for an account with the specified public key.
    /// Public keys are unique identifiers in Nostr, so this will return at most one account.
    ///
    /// # Arguments
    ///
    /// * `pubkey` - The public key of the account to find
    ///
    /// # Returns
    ///
    /// Returns the `Account` with the matching public key.
    ///
    /// # Errors
    ///
    /// This method will return an error if:
    /// - No account exists with the specified public key
    /// - There's a database connection or query error
    /// - Account data is corrupted or cannot be deserialized
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use nostr_sdk::PublicKey;
    ///
    /// // Find an account by public key
    /// let pubkey = PublicKey::from_hex("npub1...")?;
    ///
    /// match whitenoise.find_account_by_pubkey(&pubkey).await {
    ///     Ok(account) => {
    ///         println!("Found account created at: {}", account.created_at);
    ///         // Use the account for operations
    ///     }
    ///     Err(_) => {
    ///         println!("Account not found for pubkey: {}", pubkey);
    ///         // Maybe create a new account or show error
    ///     }
    /// }
    ///
    /// // Use with error propagation
    /// let account = whitenoise.find_account_by_pubkey(&pubkey).await?;
    /// let metadata = account.metadata(&whitenoise).await?;
    /// ```
    pub async fn find_account_by_pubkey(&self, pubkey: &PublicKey) -> Result<Account> {
        Account::find_by_pubkey(pubkey, &self.database).await
    }

    async fn create_base_account_with_private_key(&self, keys: &Keys) -> Result<Account> {
        let (account, _keys) = Account::new(self, Some(keys.clone())).await?;

        self.secrets_store.store_private_key(keys).map_err(|e| {
            tracing::error!(target: "whitenoise::setup_account", "Failed to store private key: {}", e);
            e
        })?;

        let account = self.persist_account(&account).await?;

        Ok(account)
    }

    async fn create_user_for_account(&self, account: &Account) -> Result<(User, bool)> {
        let result = User::find_or_create_by_pubkey(&account.pubkey, &self.database).await?;
        Ok(result)
    }

    async fn activate_account(&self, account: &Account) -> Result<()> {
        account.connect_relays(self).await?;
        tracing::debug!(target: "whitenoise::persist_and_activate_account", "Relays connected");
        self.setup_subscriptions(account).await?;
        tracing::debug!(target: "whitenoise::persist_and_activate_account", "Subscriptions setup");
        self.setup_key_package(account).await?;
        tracing::debug!(target: "whitenoise::persist_and_activate_account", "Key package setup");
        Ok(())
    }

    async fn setup_metadata(&self, account: &Account) -> Result<()> {
        let petname = petname::petname(2, " ")
            .unwrap_or_else(|| "Anonymous User".to_string())
            .split_whitespace()
            .map(Whitenoise::capitalize_first_letter)
            .collect::<Vec<_>>()
            .join(" ");

        let metadata = Metadata {
            name: Some(petname.clone()),
            display_name: Some(petname),
            ..Default::default()
        };

        account.update_metadata(&metadata, self).await?;
        tracing::debug!(target: "whitenoise::setup_metadata", "Created and published metadata with petname: {}", metadata.name.as_ref().unwrap_or(&"Unknown".to_string()));
        Ok(())
    }

    async fn persist_account(&self, account: &Account) -> Result<Account> {
        account.save(&self.database).await.map_err(|e| {
            tracing::error!(target: "whitenoise::setup_account", "Failed to save account: {}", e);
            // Try to clean up stored private key
            if let Err(cleanup_err) = self.secrets_store.remove_private_key_for_pubkey(&account.pubkey) {
                tracing::error!(target: "whitenoise::setup_account", "Failed to cleanup private key after account save failure: {}", cleanup_err);
            }
            e
        })?;
        tracing::debug!(target: "whitenoise::setup_account", "Account saved to database");
        let account = Account::find_by_pubkey(&account.pubkey, &self.database).await?;
        Ok(account)
    }

    async fn setup_key_package(&self, account: &Account) -> Result<()> {
        let relays = account.key_package_relays(self).await?;
        tracing::debug!(target: "whitenoise::setup_key_package", "Found {} key package relays", relays.len());
        let key_package_event = self
            .nostr
            .fetch_user_key_package(account.pubkey, &relays)
            .await?;
        if key_package_event.is_none() {
            self.publish_key_package_for_account(account).await?;
            tracing::debug!(target: "whitenoise::setup_account", "Published key package");
        }
        Ok(())
    }

    async fn load_default_relays(&self) -> Result<Vec<Relay>> {
        let mut default_relays = Vec::new();
        for Relay { url, .. } in Relay::defaults() {
            let relay = self.find_or_create_relay_by_url(&url).await?;
            default_relays.push(relay);
        }
        Ok(default_relays)
    }

    async fn setup_relays_for_new_account(&self, account: &mut Account) -> Result<()> {
        let default_relays = self.load_default_relays().await?;
        let keys = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?;

        // New accounts: Setup relays with defaults and always publish
        let nip65_relays = self
            .setup_new_account_relay_type(account, RelayType::Nip65, &default_relays)
            .await?;
        let inbox_relays = self
            .setup_new_account_relay_type(account, RelayType::Inbox, &default_relays)
            .await?;
        let key_package_relays = self
            .setup_new_account_relay_type(account, RelayType::KeyPackage, &default_relays)
            .await?;

        // Always publish all relay lists for new accounts
        self.publish_relay_list(&nip65_relays, RelayType::Nip65, &nip65_relays, &keys)
            .await?;
        self.publish_relay_list(&inbox_relays, RelayType::Inbox, &nip65_relays, &keys)
            .await?;
        self.publish_relay_list(
            &key_package_relays,
            RelayType::KeyPackage,
            &nip65_relays,
            &keys,
        )
        .await?;

        Ok(())
    }

    async fn setup_relays_for_existing_account(&self, account: &mut Account) -> Result<()> {
        let default_relays = self.load_default_relays().await?;
        let keys = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?;

        // Existing accounts: Try to fetch existing relay lists, use defaults as fallback
        let (nip65_relays, should_publish_nip65) = self
            .setup_existing_account_relay_type(
                account,
                RelayType::Nip65,
                &default_relays,
                &default_relays,
            )
            .await?;

        let (inbox_relays, should_publish_inbox) = self
            .setup_existing_account_relay_type(
                account,
                RelayType::Inbox,
                &nip65_relays,
                &default_relays,
            )
            .await?;

        let (key_package_relays, should_publish_key_package) = self
            .setup_existing_account_relay_type(
                account,
                RelayType::KeyPackage,
                &nip65_relays,
                &default_relays,
            )
            .await?;

        // Only publish relay lists that need publishing (when using defaults as fallback)
        if should_publish_nip65 {
            self.publish_relay_list(&nip65_relays, RelayType::Nip65, &nip65_relays, &keys)
                .await?;
        }
        if should_publish_inbox {
            self.publish_relay_list(&inbox_relays, RelayType::Inbox, &nip65_relays, &keys)
                .await?;
        }
        if should_publish_key_package {
            self.publish_relay_list(
                &key_package_relays,
                RelayType::KeyPackage,
                &nip65_relays,
                &keys,
            )
            .await?;
        }

        Ok(())
    }

    async fn setup_new_account_relay_type(
        &self,
        account: &mut Account,
        relay_type: RelayType,
        default_relays: &[Relay],
    ) -> Result<Vec<Relay>> {
        // New accounts: always use defaults (no fetching needed)
        self.add_relays_to_account(account, default_relays, relay_type)
            .await?;
        Ok(default_relays.to_vec())
    }

    async fn setup_existing_account_relay_type(
        &self,
        account: &mut Account,
        relay_type: RelayType,
        source_relays: &[Relay],
        default_relays: &[Relay],
    ) -> Result<(Vec<Relay>, bool)> {
        // Existing accounts: try to fetch existing relay lists first
        let fetched_relays = self
            .fetch_existing_relays(account.pubkey, relay_type, source_relays)
            .await?;

        if fetched_relays.is_empty() {
            // No existing relay lists - use defaults and publish
            self.add_relays_to_account(account, default_relays, relay_type)
                .await?;
            Ok((default_relays.to_vec(), true))
        } else {
            // Found existing relay lists - use them, no publishing needed
            self.add_relays_to_account(account, &fetched_relays, relay_type)
                .await?;
            Ok((fetched_relays, false))
        }
    }

    async fn fetch_existing_relays(
        &self,
        pubkey: PublicKey,
        relay_type: RelayType,
        source_relays: &[Relay],
    ) -> Result<Vec<Relay>> {
        let relay_urls = self
            .nostr
            .fetch_user_relays(pubkey, relay_type, source_relays)
            .await?;

        let mut relays = Vec::new();
        for url in relay_urls {
            let relay = self.find_or_create_relay_by_url(&url).await?;
            relays.push(relay);
        }

        Ok(relays)
    }

    async fn add_relays_to_account(
        &self,
        account: &mut Account,
        relays: &[Relay],
        relay_type: RelayType,
    ) -> Result<()> {
        for relay in relays {
            account.add_relay(relay, relay_type, self).await?;
        }
        Ok(())
    }

    async fn publish_relay_list(
        &self,
        relays: &[Relay],
        relay_type: RelayType,
        target_relays: &[Relay],
        keys: &Keys,
    ) -> Result<()> {
        self.nostr
            .publish_relay_list_with_signer(relays, relay_type, target_relays, keys.clone())
            .await?;
        Ok(())
    }

    pub(crate) async fn background_publish_account_metadata(
        &self,
        account: &Account,
    ) -> Result<()> {
        let account_clone = account.clone();
        let nostr = self.nostr.clone();
        let signer = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?;
        let user = account.user(&self.database).await?;
        let relays = account.nip65_relays(self).await?;

        tokio::spawn(async move {
            tracing::debug!(target: "whitenoise::accounts::background_publish_user_metadata", "Background task: Publishing metadata for account: {:?}", account_clone.pubkey);

            nostr
                .publish_metadata_with_signer(&user.metadata, &relays, signer)
                .await?;

            tracing::debug!(target: "whitenoise::accounts::background_publish_user_metadata", "Successfully published metadata for account: {:?}", account_clone.pubkey);
            Ok::<(), WhitenoiseError>(())
        });
        Ok(())
    }

    pub(crate) async fn background_publish_account_relay_list(
        &self,
        account: &Account,
        relay_type: RelayType,
    ) -> Result<()> {
        let account_clone = account.clone();
        let nostr = self.nostr.clone();
        let relays = account.relays(relay_type, self).await?;
        let keys = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?;
        let target_relays = if relay_type == RelayType::Nip65 {
            relays.clone()
        } else {
            account.nip65_relays(self).await?
        };

        tokio::spawn(async move {
            tracing::debug!(target: "whitenoise::accounts::background_publish_account_relay_list", "Background task: Publishing relay list for account: {:?}", account_clone.pubkey);

            nostr
                .publish_relay_list_with_signer(&relays, relay_type, &target_relays, keys)
                .await?;

            tracing::debug!(target: "whitenoise::accounts::background_publish_account_relay_list", "Successfully published relay list for account: {:?}", account_clone.pubkey);
            Ok::<(), WhitenoiseError>(())
        });
        Ok(())
    }

    pub(crate) async fn background_publish_account_follow_list(
        &self,
        account: &Account,
    ) -> Result<()> {
        let account_clone = account.clone();
        let nostr = self.nostr.clone();
        let relays = account.nip65_relays(self).await?;
        let keys = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?;
        let follows = account.follows(&self.database).await?;
        let follows_pubkeys = follows.iter().map(|f| f.pubkey).collect::<Vec<_>>();

        tokio::spawn(async move {
            tracing::debug!(target: "whitenoise::accounts::background_publish_account_follow_list", "Background task: Publishing follow list for account: {:?}", account_clone.pubkey);

            nostr
                .publish_follow_list_with_signer(&follows_pubkeys, &relays, keys)
                .await?;

            tracing::debug!(target: "whitenoise::accounts::background_publish_account_follow_list", "Successfully published follow list for account: {:?}", account_clone.pubkey);
            Ok::<(), WhitenoiseError>(())
        });
        Ok(())
    }

    pub(crate) async fn background_sync_account_data(&self, account: &Account) -> Result<()> {
        let group_ids = account.load_nostr_group_ids(self)?;
        let nostr = self.nostr.clone();
        let database = self.database.clone();
        let account_clone = account.clone();
        let signer = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?;

        tokio::spawn(async move {
            tracing::debug!(
                target: "whitenoise::background_fetch_account_data",
                "Starting background fetch for account: {}",
                account_clone.pubkey.to_hex()
            );

            let current_time = Utc::now().timestamp_millis();
            match nostr
                .sync_all_user_data(signer, &account_clone, group_ids)
                .await
            {
                Ok(_) => {
                    // Update the last_synced timestamp in the database
                    if let Err(e) =
                        sqlx::query("UPDATE accounts SET last_synced_at = ? WHERE pubkey = ?")
                            .bind(current_time)
                            .bind(account_clone.pubkey.to_hex())
                            .execute(&database.pool)
                            .await
                    {
                        tracing::error!(
                            target: "whitenoise::background_fetch_account_data",
                            "Failed to update last_synced timestamp for account {}: {}",
                            account_clone.pubkey.to_hex(),
                            e
                        );
                    } else {
                        tracing::info!(
                            target: "whitenoise::background_fetch_account_data",
                            "Successfully fetched data and updated last_synced for account: {}",
                            account_clone.pubkey.to_hex()
                        );
                    }
                }
                Err(e) => {
                    tracing::error!(
                        target: "whitenoise::background_fetch_account_data",
                        "Failed to fetch user data for account {}: {}",
                        account_clone.pubkey.to_hex(),
                        e
                    );
                }
            }
        });

        Ok(())
    }

    pub(crate) async fn setup_subscriptions(&self, account: &Account) -> Result<()> {
        tracing::debug!(
            target: "whitenoise::setup_subscriptions",
            "Setting up subscriptions for account: {:?}",
            account
        );
        let mut group_relays = HashSet::new();
        let groups: Vec<group_types::Group>;
        {
            let nostr_mls = Account::create_nostr_mls(account.pubkey, &self.config.data_dir)?;
            groups = nostr_mls.get_groups()?;
            // Collect all relays from all groups into a single vector
            for group in &groups {
                let relays = nostr_mls.get_relays(&group.mls_group_id)?;
                for relay in relays {
                    group_relays.insert(relay.clone());
                }
            }
        };
        tracing::debug!(
            target: "whitenoise::setup_subscriptions",
            "Found {} groups",
            groups.len()
        );
        // We do this in two stages to deduplicate the relays
        let mut group_relays_vec = Vec::new();
        for relay in group_relays {
            group_relays_vec.push(Relay::find_or_create_by_url(&relay, &self.database).await?);
        }

        tracing::debug!(
            target: "whitenoise::setup_subscriptions",
            "Found {} group relays",
            group_relays_vec.len()
        );

        let nostr_group_ids = groups
            .into_iter()
            .map(|group| hex::encode(group.nostr_group_id))
            .collect::<Vec<String>>();

        // Use the signer-aware subscription setup method
        let keys = self
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?;

        let user_relays = account.nip65_relays(self).await?;
        tracing::debug!(
            target: "whitenoise::setup_subscriptions",
            "About to setup account subscriptions with user relays: {:?}",
            user_relays
        );
        self.nostr
            .setup_account_subscriptions_with_signer(
                account.pubkey,
                &account.nip65_relays(self).await?,
                &account.inbox_relays(self).await?,
                &group_relays_vec,
                &nostr_group_ids,
                keys,
            )
            .await?;

        tracing::debug!(
            target: "whitenoise::setup_subscriptions",
            "Subscriptions setup"
        );
        Ok(())
    }
}

#[cfg(test)]
pub mod test_utils {
    use nostr::key::PublicKey;
    use nostr_mls::NostrMls;
    use nostr_mls_sqlite_storage::NostrMlsSqliteStorage;
    use std::path::PathBuf;
    use tempfile::TempDir;

    pub fn data_dir() -> PathBuf {
        TempDir::new().unwrap().path().to_path_buf()
    }

    pub fn create_nostr_mls(pubkey: PublicKey) -> NostrMls<NostrMlsSqliteStorage> {
        super::Account::create_nostr_mls(pubkey, &data_dir()).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::whitenoise::accounts::Account;
    use crate::whitenoise::test_utils::*;

    #[tokio::test]
    #[ignore]
    async fn test_login_after_delete_all_data() {
        let whitenoise = test_get_whitenoise().await;

        let account = setup_login_account(whitenoise).await;
        whitenoise.delete_all_data().await.unwrap();
        let _acc = whitenoise
            .login(account.1.secret_key().to_secret_hex())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_load_accounts() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Test loading empty database
        let accounts = Account::all(&whitenoise.database).await.unwrap();
        assert!(accounts.is_empty());

        // Create test accounts and save them to database
        let (account1, keys1) = create_test_account(&whitenoise).await;
        let (account2, keys2) = create_test_account(&whitenoise).await;

        // Save accounts to database
        account1.save(&whitenoise.database).await.unwrap();
        account2.save(&whitenoise.database).await.unwrap();

        // Store keys in secrets store (required for background fetch)
        whitenoise.secrets_store.store_private_key(&keys1).unwrap();
        whitenoise.secrets_store.store_private_key(&keys2).unwrap();

        // Load accounts from database
        let loaded_accounts = Account::all(&whitenoise.database).await.unwrap();
        assert_eq!(loaded_accounts.len(), 2);
        let pubkeys: Vec<PublicKey> = loaded_accounts.iter().map(|a| a.pubkey).collect();
        assert!(pubkeys.contains(&account1.pubkey));
        assert!(pubkeys.contains(&account2.pubkey));

        // Verify account data is correctly loaded
        let loaded_account1 = loaded_accounts
            .iter()
            .find(|a| a.pubkey == account1.pubkey)
            .unwrap();
        assert_eq!(loaded_account1.pubkey, account1.pubkey);
        assert_eq!(loaded_account1.user_id, account1.user_id);
        assert_eq!(loaded_account1.last_synced_at, account1.last_synced_at);
        // Allow for small precision differences in timestamps due to database storage
        let created_diff = (loaded_account1.created_at - account1.created_at)
            .num_milliseconds()
            .abs();
        let updated_diff = (loaded_account1.updated_at - account1.updated_at)
            .num_milliseconds()
            .abs();
        assert!(
            created_diff <= 1,
            "Created timestamp difference too large: {}ms",
            created_diff
        );
        assert!(
            updated_diff <= 1,
            "Updated timestamp difference too large: {}ms",
            updated_diff
        );
    }

    #[tokio::test]
    async fn test_create_identity_publishes_relay_lists() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create a new identity
        let account = whitenoise.create_identity().await.unwrap();

        // Give the events time to be published and processed
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Check that all three event types were published
        let inbox_events = whitenoise
            .nostr
            .fetch_user_relays(
                account.pubkey,
                RelayType::Inbox,
                &account.nip65_relays(&whitenoise).await.unwrap(),
            )
            .await
            .unwrap();

        let key_package_relays_events = whitenoise
            .nostr
            .fetch_user_relays(
                account.pubkey,
                RelayType::KeyPackage,
                &account.nip65_relays(&whitenoise).await.unwrap(),
            )
            .await
            .unwrap();

        let key_package_events = whitenoise
            .nostr
            .fetch_user_key_package(
                account.pubkey,
                &account.nip65_relays(&whitenoise).await.unwrap(),
            )
            .await
            .unwrap();

        // Verify that the relay list events were published
        assert!(
            !inbox_events.is_empty(),
            "Inbox relays list (kind 10050) should be published for new accounts"
        );
        assert!(
            !key_package_relays_events.is_empty(),
            "Key package relays list (kind 10051) should be published for new accounts"
        );
        assert!(
            key_package_events.is_some(),
            "Key package (kind 443) should be published for new accounts"
        );
    }

    /// Helper function to verify that an account has all three relay lists properly configured
    async fn verify_account_relay_lists_setup(whitenoise: &Whitenoise, account: &Account) {
        // Verify all three relay lists are set up with default relays
        let default_relays = Relay::defaults();
        let default_relay_count = default_relays.len();

        // Check relay database state
        assert_eq!(
            account.nip65_relays(whitenoise).await.unwrap().len(),
            default_relay_count,
            "Account should have default NIP-65 relays configured"
        );
        assert_eq!(
            account.inbox_relays(whitenoise).await.unwrap().len(),
            default_relay_count,
            "Account should have default inbox relays configured"
        );
        assert_eq!(
            account.key_package_relays(whitenoise).await.unwrap().len(),
            default_relay_count,
            "Account should have default key package relays configured"
        );

        let default_relays_vec: Vec<RelayUrl> =
            default_relays.iter().map(|r| r.url.clone()).collect();
        let nip65_relay_urls: Vec<RelayUrl> = account
            .nip65_relays(whitenoise)
            .await
            .unwrap()
            .iter()
            .map(|r| r.url.clone())
            .collect();
        let inbox_relay_urls: Vec<RelayUrl> = account
            .inbox_relays(whitenoise)
            .await
            .unwrap()
            .iter()
            .map(|r| r.url.clone())
            .collect();
        let key_package_relay_urls: Vec<RelayUrl> = account
            .key_package_relays(whitenoise)
            .await
            .unwrap()
            .iter()
            .map(|r| r.url.clone())
            .collect();
        for default_relay in default_relays_vec.iter() {
            assert!(
                nip65_relay_urls.contains(default_relay),
                "NIP-65 relays should contain default relay: {}",
                default_relay
            );
            assert!(
                inbox_relay_urls.contains(default_relay),
                "Inbox relays should contain default relay: {}",
                default_relay
            );
            assert!(
                key_package_relay_urls.contains(default_relay),
                "Key package relays should contain default relay: {}",
                default_relay
            );
        }
    }

    /// Helper function to verify that an account has a key package published
    async fn verify_account_key_package_exists(whitenoise: &Whitenoise, account: &Account) {
        // Check if key package exists by trying to fetch it
        let key_package_event = whitenoise
            .nostr
            .fetch_user_key_package(
                account.pubkey,
                &account.key_package_relays(whitenoise).await.unwrap(),
            )
            .await
            .unwrap();

        assert!(
            key_package_event.is_some(),
            "Account should have a key package published to relays"
        );

        // If key package exists, verify it's authored by the correct account
        if let Some(event) = key_package_event {
            assert_eq!(
                event.pubkey, account.pubkey,
                "Key package should be authored by the account's public key"
            );
            assert_eq!(
                event.kind,
                Kind::MlsKeyPackage,
                "Event should be a key package (kind 443)"
            );
        }
    }

    #[tokio::test]
    async fn test_create_identity_sets_up_all_requirements() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create a new identity
        let account = whitenoise.create_identity().await.unwrap();

        // Give the events time to be published and processed
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Verify all three relay lists are properly configured
        verify_account_relay_lists_setup(&whitenoise, &account).await;

        // Verify key package is published
        verify_account_key_package_exists(&whitenoise, &account).await;
    }

    #[tokio::test]
    async fn test_login_existing_account_sets_up_all_requirements() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create an account through login (simulating an existing account)
        let keys = create_test_keys();
        let account = whitenoise
            .login(keys.secret_key().to_secret_hex())
            .await
            .unwrap();

        // Give the events time to be published and processed
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Verify all three relay lists are properly configured
        verify_account_relay_lists_setup(&whitenoise, &account).await;

        // Verify key package is published
        verify_account_key_package_exists(&whitenoise, &account).await;
    }

    #[tokio::test]
    async fn test_login_with_existing_relay_lists_preserves_them() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // First, create an account and let it publish relay lists
        let keys = create_test_keys();
        let account1 = whitenoise
            .login(keys.secret_key().to_secret_hex())
            .await
            .unwrap();

        // Give time for initial setup
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Verify initial setup is correct
        verify_account_relay_lists_setup(&whitenoise, &account1).await;
        verify_account_key_package_exists(&whitenoise, &account1).await;

        // Logout the account
        whitenoise.logout(&account1.pubkey).await.unwrap();

        // Login again with the same keys (simulating returning user)
        let account2 = whitenoise
            .login(keys.secret_key().to_secret_hex())
            .await
            .unwrap();

        // Give time for login process
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Verify that relay lists are still properly configured
        verify_account_relay_lists_setup(&whitenoise, &account2).await;

        // Verify key package still exists (should not publish a new one)
        verify_account_key_package_exists(&whitenoise, &account2).await;

        // Accounts should be equivalent (same pubkey, same basic setup)
        assert_eq!(
            account1.pubkey, account2.pubkey,
            "Same keys should result in same account"
        );
    }

    #[tokio::test]
    async fn test_multiple_accounts_each_have_proper_setup() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create multiple accounts
        let mut accounts = Vec::new();
        for i in 0..3 {
            let keys = create_test_keys();
            let account = whitenoise
                .login(keys.secret_key().to_secret_hex())
                .await
                .unwrap();
            accounts.push((account, keys));

            tracing::info!("Created account {}: {}", i, accounts[i].0.pubkey.to_hex());
        }

        // Give time for all accounts to be set up
        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

        // Verify each account has proper setup
        for (i, (account, _)) in accounts.iter().enumerate() {
            tracing::info!("Verifying account {}: {}", i, account.pubkey.to_hex());

            // Verify all three relay lists are properly configured
            verify_account_relay_lists_setup(&whitenoise, account).await;

            // Verify key package is published
            verify_account_key_package_exists(&whitenoise, account).await;
        }

        // Verify accounts are distinct
        for i in 0..accounts.len() {
            for j in i + 1..accounts.len() {
                assert_ne!(
                    accounts[i].0.pubkey, accounts[j].0.pubkey,
                    "Each account should have a unique public key"
                );
            }
        }
    }

    #[tokio::test]
    async fn test_update_metadata() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let (account, keys) = create_test_account(&whitenoise).await;
        account.save(&whitenoise.database).await.unwrap();

        whitenoise.secrets_store.store_private_key(&keys).unwrap();

        let default_relays = whitenoise.load_default_relays().await.unwrap();
        for relay in &default_relays {
            account
                .add_relay(relay, RelayType::Nip65, &whitenoise)
                .await
                .unwrap();
        }

        let test_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let new_metadata = Metadata::new()
            .name(format!("updated_user_{}", test_timestamp))
            .display_name(format!("Updated User {}", test_timestamp))
            .about("Updated metadata for testing");

        let result = account.update_metadata(&new_metadata, &whitenoise).await;
        result.expect("Failed to update metadata. Are test relays running on localhost:8080 and localhost:7777?");

        let user = account.user(&whitenoise.database).await.unwrap();
        assert_eq!(user.metadata.name, new_metadata.name);
        assert_eq!(user.metadata.display_name, new_metadata.display_name);
        assert_eq!(user.metadata.about, new_metadata.about);

        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        let nip65_relays = account.nip65_relays(&whitenoise).await.unwrap();
        let fetched_metadata = whitenoise
            .nostr
            .fetch_metadata_from(&nip65_relays, account.pubkey)
            .await
            .expect("Failed to fetch metadata from relays");

        if let Some(published_metadata) = fetched_metadata {
            assert_eq!(published_metadata.name, new_metadata.name);
            assert_eq!(published_metadata.display_name, new_metadata.display_name);
            assert_eq!(published_metadata.about, new_metadata.about);
        }
    }
}
