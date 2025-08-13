use crate::types::ImageType;
use crate::whitenoise::error::{Result, WhitenoiseError};
use crate::whitenoise::Whitenoise;
use crate::Account;
use dashmap::DashSet;
use nostr_blossom::prelude::*;
use nostr_sdk::prelude::*;

impl Whitenoise {


}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::whitenoise::test_utils::*;

    #[tokio::test]
    async fn test_upload_profile_picture() {
        use base64::prelude::*;

        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create and save a test account
        let (account, keys) = create_test_account();
        whitenoise.save_account(&account).await.unwrap();
        whitenoise.secrets_store.store_private_key(&keys).unwrap();

        // Login to the account so that logged_in() returns true
        let log_account = whitenoise.login(keys.secret_key().to_secret_hex()).await;
        assert!(log_account.is_ok());
        assert_eq!(log_account.unwrap(), account);

        // Create initial metadata so that upload_profile_picture can update it
        let initial_metadata = Metadata {
            name: Some("Test User".to_string()),
            ..Default::default()
        };
        whitenoise
            .update_metadata(&initial_metadata, &account)
            .await
            .unwrap();

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

        // Verify we got a URL back
        let image_url = result.unwrap();
        assert!(image_url.starts_with("http"), "Should return a valid URL");
        assert!(
            image_url.contains("localhost:3000"),
            "Should use the specified server"
        );
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
        let result = whitenoise.update_metadata(&metadata, &account).await;
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

        // Create minimal metadata (only name)
        let metadata = Metadata {
            name: Some("Simple Name".to_string()),
            ..Default::default()
        };

        // Test updating metadata
        let result = whitenoise.update_metadata(&metadata, &account).await;
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

        // Create completely empty metadata
        let metadata = Metadata::default();

        // Test updating metadata
        let result = whitenoise.update_metadata(&metadata, &account).await;
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
        let result = whitenoise.update_metadata(&metadata, &account).await;
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

    #[tokio::test]
    async fn test_fetch_metadata_from_cache() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create and save a test account
        let (account, keys) = create_test_account();
        whitenoise.save_account(&account).await.unwrap();
        whitenoise.secrets_store.store_private_key(&keys).unwrap();

        // Login to the account
        let log_account = whitenoise.login(keys.secret_key().to_secret_hex()).await;
        assert!(log_account.is_ok());
        assert_eq!(log_account.unwrap(), account);

        // Create test metadata and store it in the nostr database
        let test_metadata = Metadata {
            name: Some("Test User".to_string()),
            display_name: Some("Test Display Name".to_string()),
            about: Some("Test bio".to_string()),
            picture: Some("https://example.com/avatar.jpg".to_string()),
            ..Default::default()
        };

        // First publish the metadata so it gets stored in the local database
        whitenoise
            .update_metadata(&test_metadata, &account)
            .await
            .unwrap();

        // Wait a bit for the metadata to be processed
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Now fetch the metadata - this should come from the cache
        let result = whitenoise
            .fetch_metadata_from(account.nip65_relays.clone(), account.pubkey)
            .await;
        assert!(result.is_ok(), "fetch_metadata should succeed");

        let metadata = result.unwrap();
        assert!(metadata.is_some(), "metadata should be found");

        let retrieved_metadata = metadata.unwrap();
        assert_eq!(retrieved_metadata.name, test_metadata.name);
        assert_eq!(retrieved_metadata.display_name, test_metadata.display_name);
        assert_eq!(retrieved_metadata.about, test_metadata.about);
        assert_eq!(retrieved_metadata.picture, test_metadata.picture);
    }

    #[tokio::test]
    async fn test_fetch_metadata_from_relays() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create and save a test account
        let (account, keys) = create_test_account();
        whitenoise.save_account(&account).await.unwrap();
        whitenoise.secrets_store.store_private_key(&keys).unwrap();

        // Login to the account
        let log_account = whitenoise.login(keys.secret_key().to_secret_hex()).await;
        assert!(log_account.is_ok());
        assert_eq!(log_account.unwrap(), account);

        // Create a second account whose metadata we'll try to fetch
        let (other_account, other_keys) = create_test_account();
        whitenoise.save_account(&other_account).await.unwrap();
        whitenoise
            .secrets_store
            .store_private_key(&other_keys)
            .unwrap();

        // Login to the other account temporarily to publish metadata
        let other_log_account = whitenoise
            .login(other_keys.secret_key().to_secret_hex())
            .await;
        assert!(other_log_account.is_ok());

        // Publish metadata for the other account
        let other_metadata = Metadata {
            name: Some("Other User".to_string()),
            display_name: Some("Other Display Name".to_string()),
            about: Some("Other bio".to_string()),
            ..Default::default()
        };

        whitenoise
            .update_metadata(&other_metadata, &other_account)
            .await
            .unwrap();

        // Wait for the metadata to be published
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        // Switch back to the original account
        let log_account = whitenoise.login(keys.secret_key().to_secret_hex()).await;
        assert!(log_account.is_ok());

        // Now try to fetch the other account's metadata
        // This should attempt to fetch from relays since it's not in our local cache
        let result = whitenoise
            .fetch_metadata_from(account.nip65_relays.clone(), other_account.pubkey)
            .await;
        assert!(result.is_ok(), "fetch_metadata should succeed");

        // Note: In a real test environment, this might return None if the relay fetch fails
        // or if the local nostr database doesn't have the metadata yet
        let metadata = result.unwrap();
        if let Some(retrieved_metadata) = metadata {
            assert_eq!(retrieved_metadata.name, other_metadata.name);
            assert_eq!(retrieved_metadata.display_name, other_metadata.display_name);
            assert_eq!(retrieved_metadata.about, other_metadata.about);
        }
        // We don't assert that metadata is Some() because relay fetching might not work
        // in the test environment, but the method should still succeed
    }

    #[tokio::test]
    async fn test_fetch_metadata_not_found() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create and save a test account
        let (account, keys) = create_test_account();
        whitenoise.save_account(&account).await.unwrap();
        whitenoise.secrets_store.store_private_key(&keys).unwrap();

        // Login to the account
        let log_account = whitenoise.login(keys.secret_key().to_secret_hex()).await;
        assert!(log_account.is_ok());
        assert_eq!(log_account.unwrap(), account);

        // Create a random public key that doesn't exist
        let random_keys = Keys::generate();
        let random_pubkey = random_keys.public_key();

        // Try to fetch metadata for a non-existent user
        let result = whitenoise
            .fetch_metadata_from(account.nip65_relays.clone(), random_pubkey)
            .await;
        assert!(
            result.is_ok(),
            "fetch_metadata should succeed even when no metadata is found"
        );

        let metadata = result.unwrap();
        assert!(
            metadata.is_none(),
            "metadata should be None for non-existent user"
        );
    }

    #[tokio::test]
    async fn test_fetch_metadata_for_different_user() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create and save a test account
        let (account, keys) = create_test_account();
        whitenoise.save_account(&account).await.unwrap();
        whitenoise.secrets_store.store_private_key(&keys).unwrap();

        // Login to the account
        let log_account = whitenoise.login(keys.secret_key().to_secret_hex()).await;
        assert!(log_account.is_ok());
        assert_eq!(log_account.unwrap(), account);

        // Create a different user's keys
        let other_keys = Keys::generate();
        let other_pubkey = other_keys.public_key();

        // Try to fetch metadata for the other user
        let result = whitenoise
            .fetch_metadata_from(account.nip65_relays.clone(), other_pubkey)
            .await;
        assert!(
            result.is_ok(),
            "fetch_metadata should succeed for different user"
        );

        let metadata = result.unwrap();
        // The metadata should be None since the other user doesn't exist in our test setup
        assert!(
            metadata.is_none(),
            "metadata should be None for user without metadata"
        );
    }
}
