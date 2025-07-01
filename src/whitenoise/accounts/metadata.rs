use crate::types::ImageType;
use crate::whitenoise::error::{Result, WhitenoiseError};
use crate::whitenoise::Whitenoise;
use crate::RelayType;
use nostr::hashes::sha256::Hash as Sha256Hash;
use nostr_blossom::prelude::*;
use nostr_sdk::prelude::*;

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

    /// Updates the metadata for the given account by publishing a new metadata event to Nostr.
    ///
    /// This method takes the provided metadata, creates a Nostr metadata event (Kind::Metadata),
    /// and publishes it to the account's relays. It also updates the account's `last_synced` timestamp
    /// in the database to reflect the successful publication.
    ///
    /// # Arguments
    ///
    /// * `metadata` - The new `Metadata` to publish for the account.
    /// * `pubkey` - A reference to the `PublicKey` of an `Account` whose metadata should be updated.
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
    pub async fn update_metadata(&self, metadata: &Metadata, pubkey: &PublicKey) -> Result<()> {
        if !self.logged_in(pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }

        tracing::debug!(
            target: "whitenoise::update_metadata",
            "Updating metadata for account: {}",
            pubkey.to_hex()
        );

        // Serialize metadata to JSON
        let metadata_json = serde_json::to_string(metadata)?;

        // Create metadata event
        let event = EventBuilder::new(Kind::Metadata, metadata_json);

        // Get signing keys for the account
        let keys = self.secrets_store.get_nostr_keys_for_pubkey(pubkey)?;

        // Get relays with fallback to defaults if user hasn't configured any
        let relays_to_use = self
            .fetch_relays_with_fallback(*pubkey, RelayType::Nostr)
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

    /// Uploads a profile picture to a Blossom server and updates the account settings.
    ///
    /// This method performs the following steps:
    /// 1. Creates a Blossom client for the specified server
    /// 2. Retrieves the user's Nostr keys for authentication
    /// 3. Reads the image file from the filesystem
    /// 4. Uploads the image blob to the Blossom server with the appropriate content type
    /// 5. Updates the account settings with the returned blob descriptor
    ///
    /// The uploaded image becomes the user's profile picture and is referenced by the
    /// blob descriptor stored in their account settings. The Blossom protocol provides
    /// content-addressable storage, ensuring the image can be retrieved by its hash.
    ///
    /// # Arguments
    ///
    /// * `pubkey` - A reference to the `PublicKey` of the account uploading the profile picture
    /// * `server` - The `Url` of the Blossom server to upload to
    /// * `file_path` - `&str` pointing to the image file to be uploaded
    /// * `image_type` - The `ImageType` enum specifying the image format (JPG, JPEG, or PNG)
    ///
    /// # Returns
    ///
    /// Returns Ok(Sha256Hash) of the image uploaded to blossom server
    ///
    /// # Errors
    ///
    /// Returns a `WhitenoiseError` if:
    /// * The account is not found or not logged in
    /// * The user's Nostr keys cannot be retrieved from the secrets store
    /// * The image file cannot be read from the filesystem
    /// * The upload to the Blossom server fails (network error, authentication failure, etc.)
    /// * The account settings cannot be fetched from the database
    /// * The account settings cannot be updated in the database
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use url::Url;
    /// use crate::types::ImageType;
    ///
    /// let server_url = Url::parse("http://localhost:3000").unwrap();
    /// let image_path = "./profile.png";
    ///
    /// whitenoise.upload_profile_picture(
    ///     &user_pubkey,
    ///     server_url,
    ///     image_path,
    ///     ImageType::PNG
    /// ).await?;
    /// ```
    pub async fn upload_profile_picture(
        &self,
        pubkey: PublicKey,
        server: Url,
        file_path: &str,
        image_type: ImageType,
    ) -> Result<Sha256Hash> {
        if !self.logged_in(&pubkey).await {
            return Err(WhitenoiseError::AccountNotFound);
        }
        let client = BlossomClient::new(server);
        let keys = self.secrets_store.get_nostr_keys_for_pubkey(&pubkey)?;
        let data = tokio::fs::read(file_path).await?;

        let descriptor = client
            .upload_blob(data, Some(image_type.content_type()), None, Some(&keys))
            .await
            .map_err(|err| WhitenoiseError::Other(anyhow::anyhow!(err)))?;

        // Publish updated MetaData event
        let some_metadata = self.fetch_metadata(pubkey).await?;
        let mut metadata = some_metadata.ok_or(WhitenoiseError::AccountNotAuthorized)?;

        metadata.picture = Some(descriptor.url.to_string());

        self.update_metadata(&metadata, &pubkey).await?;

        Ok(descriptor.sha256)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::whitenoise::test_utils::*;

    #[tokio::test]
    #[ignore]
    async fn test_upload_profile_picture() {
        use base64::prelude::*;

        let whitenoise = test_get_whitenoise().await;
        let (mut account, keys) = setup_login_account(whitenoise).await;
        whitenoise.onboard_new_account(&mut account).await.unwrap();

        let img_data = b"iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";
        let img_bytes = BASE64_STANDARD.decode(img_data).unwrap();

        let dir_path = ".test";
        let file_path = ".test/fake_image.png";

        // 1. Create directory if it doesn't exist
        if !Path::new(dir_path).exists() {
            tokio::fs::create_dir(dir_path).await.unwrap();
        }
        tokio::fs::write(file_path, &img_bytes).await.unwrap();

        let server_url = url::Url::parse("http://localhost:3000").unwrap();

        let result = whitenoise
            .upload_profile_picture(
                account.pubkey,
                server_url.clone(),
                file_path,
                crate::types::ImageType::Png,
            )
            .await;
        assert!(result.is_ok(), "{result:?}");

        // Check if the blob is available in the blossom server
        let hash = result.unwrap();
        let client = BlossomClient::new(server_url);
        assert!(client.has_blob(hash, None, Some(&keys)).await.unwrap());
    }

    #[tokio::test]
    async fn test_update_metadata() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create and save a test account
        let (account, test_keys) = create_test_account();
        whitenoise.save_account(&account).await.unwrap();
        whitenoise
            .secrets_store
            .store_private_key(&test_keys)
            .unwrap();

        let log_account = whitenoise
            .login(test_keys.secret_key().to_secret_hex())
            .await;
        assert!(log_account.is_ok());
        assert_eq!(log_account.unwrap(), account);

        // Initialize NostrMls for the account
        whitenoise
            .initialize_nostr_mls_for_account(&account)
            .await
            .unwrap();

        // Create test metadata
        let metadata = Metadata {
            name: Some("Updated Name".to_string()),
            display_name: Some("Updated Display Name".to_string()),
            about: Some("Updated bio".to_string()),
            picture: Some("https://example.com/new-avatar.jpg".to_string()),
            banner: Some("https://example.com/banner.jpg".to_string()),
            nip05: Some("user@example.com".to_string()),
            lud16: Some("user@lightning.example.com".to_string()),
            ..Default::default()
        };

        // Test updating metadata
        let result = whitenoise.update_metadata(&metadata, &account.pubkey).await;
        assert!(result.is_ok(), "update_metadata should succeed");
    }

    #[tokio::test]
    async fn test_update_metadata_with_minimal_metadata() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create and save a test account
        let (account, keys) = create_test_account();
        whitenoise.save_account(&account).await.unwrap();
        whitenoise.secrets_store.store_private_key(&keys).unwrap();
        let log_account = whitenoise.login(keys.secret_key().to_secret_hex()).await;
        assert!(log_account.is_ok());
        assert_eq!(log_account.unwrap(), account);

        // Initialize NostrMls for the account
        whitenoise
            .initialize_nostr_mls_for_account(&account)
            .await
            .unwrap();

        // Create minimal metadata (only name)
        let metadata = Metadata {
            name: Some("Simple Name".to_string()),
            ..Default::default()
        };

        // Test updating metadata
        let result = whitenoise.update_metadata(&metadata, &account.pubkey).await;
        assert!(
            result.is_ok(),
            "update_metadata should succeed with minimal metadata"
        );
    }

    #[tokio::test]
    async fn test_update_metadata_with_empty_metadata() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create and save a test account
        let (account, keys) = create_test_account();
        whitenoise.save_account(&account).await.unwrap();
        whitenoise.secrets_store.store_private_key(&keys).unwrap();
        let log_account = whitenoise.login(keys.secret_key().to_secret_hex()).await;
        assert!(log_account.is_ok());
        assert_eq!(log_account.unwrap(), account);

        // Initialize NostrMls for the account
        whitenoise
            .initialize_nostr_mls_for_account(&account)
            .await
            .unwrap();

        // Create completely empty metadata
        let metadata = Metadata::default();

        // Test updating metadata
        let result = whitenoise.update_metadata(&metadata, &account.pubkey).await;
        assert!(
            result.is_ok(),
            "update_metadata should succeed with empty metadata"
        );
    }

    #[tokio::test]
    async fn test_update_metadata_without_stored_keys() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create and save a test account but DON'T store the keys
        let (account, _keys) = create_test_account();
        whitenoise.save_account(&account).await.unwrap();
        // Note: not storing keys in secrets_store

        // Create test metadata
        let metadata = Metadata {
            name: Some("Test Name".to_string()),
            ..Default::default()
        };

        // Test updating metadata - this should fail because keys aren't stored
        let result = whitenoise.update_metadata(&metadata, &account.pubkey).await;
        assert!(
            result.is_err(),
            "update_metadata should fail when keys aren't stored"
        );
    }

    #[tokio::test]
    async fn test_update_metadata_serialization() {
        // Test that various metadata fields serialize correctly
        let metadata = Metadata {
            name: Some("Test User".to_string()),
            display_name: Some("Test Display".to_string()),
            about: Some("Bio with special chars: émojí 🚀".to_string()),
            picture: Some("https://example.com/picture.jpg".to_string()),
            banner: Some("https://example.com/banner.jpg".to_string()),
            nip05: Some("test@example.com".to_string()),
            lud16: Some("test@lightning.example.com".to_string()),
            website: Some("https://example.com".to_string()),
            ..Default::default()
        };

        // Test that the metadata can be serialized to JSON
        let serialized = serde_json::to_string(&metadata);
        assert!(serialized.is_ok(), "Metadata should serialize to JSON");

        let json_str = serialized.unwrap();
        assert!(json_str.contains("Test User"));
        assert!(json_str.contains("Bio with special chars"));
        assert!(json_str.contains("émojí 🚀"));
    }
}
