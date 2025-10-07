use crate::whitenoise::{
    database::{Database, media_files::MediaFileParams},
    error::Result,
    storage::Storage,
};
use mdk_core::{
    encrypted_media::types::EncryptedMediaUpload, extension::group_image::GroupImageUpload, GroupId,
};
use nostr_sdk::PublicKey;
use std::path::{Path, PathBuf};

/// Intermediate type for media file storage operations
///
/// This type abstracts over different MDK upload types (GroupImageUpload, EncryptedMediaUpload)
/// and provides a unified interface for storing media files.
pub(crate) struct MediaFileUpload<'a> {
    /// The decrypted file data to store
    pub data: &'a [u8],
    /// Hash of the encrypted file (SHA-256)
    pub file_hash: [u8; 32],
    /// MIME type of the file
    pub mime_type: &'a str,
    /// Type of media (e.g., "group_image", "chat_media")
    pub media_type: &'a str,
    /// Optional Blossom URL where the encrypted file is stored
    pub blossom_url: Option<&'a str>,
    /// Optional dimensions string (e.g., "1920x1080")
    pub dimensions: Option<&'a str>,
    /// Optional blurhash string
    pub blurhash: Option<&'a str>,
}

impl<'a> From<(&'a GroupImageUpload, &'a [u8], &'a str, &'a str)> for MediaFileUpload<'a> {
    fn from(
        (upload, decrypted_data, mime_type, blossom_url): (
            &'a GroupImageUpload,
            &'a [u8],
            &'a str,
            &'a str,
        ),
    ) -> Self {
        Self {
            data: decrypted_data,
            file_hash: upload.encrypted_hash,
            mime_type,
            media_type: "group_image",
            blossom_url: Some(blossom_url),
            dimensions: None,
            blurhash: None,
        }
    }
}

impl<'a> From<(&'a EncryptedMediaUpload, &'a [u8], &'a str, &'a str)> for MediaFileUpload<'a> {
    fn from(
        (upload, decrypted_data, mime_type, blossom_url): (
            &'a EncryptedMediaUpload,
            &'a [u8],
            &'a str,
            &'a str,
        ),
    ) -> Self {
        Self {
            data: decrypted_data,
            file_hash: upload.encrypted_hash,
            mime_type,
            media_type: "chat_media",
            blossom_url: Some(blossom_url),
            dimensions: None,
            blurhash: None,
        }
    }
}

/// High-level media files orchestration layer
///
/// This module provides convenience methods that coordinate between:
/// - Storage layer (filesystem operations)
/// - Database layer (metadata tracking)
/// - Business logic (validation, coordination)
///
/// It does NOT handle:
/// - Network operations (use BlossomClient)
/// - Encryption/decryption (caller's responsibility)
pub struct MediaFiles<'a> {
    storage: &'a Storage,
    database: &'a Database,
}

impl<'a> MediaFiles<'a> {
    /// Creates a new MediaFiles orchestrator
    ///
    /// # Arguments
    /// * `storage` - Reference to the storage layer
    /// * `database` - Reference to the database
    pub(crate) fn new(storage: &'a Storage, database: &'a Database) -> Self {
        Self { storage, database }
    }

    /// Stores a file and records it in the database in one operation
    ///
    /// This is a convenience method that:
    /// 1. Stores the file to the filesystem
    /// 2. Records the metadata in the database
    ///
    /// # Arguments
    /// * `account_pubkey` - The account accessing this file
    /// * `group_id` - The MLS group ID
    /// * `subdirectory` - Subdirectory within the group (e.g., "group_images", "media")
    /// * `filename` - The filename to store as
    /// * `upload` - MediaFileUpload containing file data and metadata
    ///
    /// # Returns
    /// The path to the stored file
    pub(crate) async fn store_and_record(
        &self,
        account_pubkey: &PublicKey,
        group_id: &GroupId,
        subdirectory: &str,
        filename: &str,
        upload: MediaFileUpload<'_>,
    ) -> Result<PathBuf> {
        // Store file to filesystem
        let file_path = self
            .storage
            .media_files
            .store_file(group_id, subdirectory, filename, upload.data)
            .await?;

        // Record in database
        use crate::whitenoise::database::media_files::MediaFile;
        MediaFile::save(
            self.database,
            group_id,
            account_pubkey,
            MediaFileParams {
                file_path: &file_path,
                file_hash: &upload.file_hash,
                mime_type: upload.mime_type,
                media_type: upload.media_type,
                blossom_url: upload.blossom_url,
                dimensions: upload.dimensions,
                blurhash: upload.blurhash,
            },
        )
        .await?;

        Ok(file_path)
    }

    /// Records an existing file in the database
    ///
    /// Use this when you already have a file stored and just need to update/record metadata.
    ///
    /// # Arguments
    /// * `account_pubkey` - The account accessing this file
    /// * `group_id` - The MLS group ID
    /// * `file_path` - Path to the cached file
    /// * `upload` - MediaFileUpload containing file metadata
    pub(crate) async fn record_in_database(
        &self,
        account_pubkey: &PublicKey,
        group_id: &GroupId,
        file_path: &Path,
        upload: MediaFileUpload<'_>,
    ) -> Result<()> {
        use crate::whitenoise::database::media_files::MediaFile;

        MediaFile::save(
            self.database,
            group_id,
            account_pubkey,
            MediaFileParams {
                file_path,
                file_hash: &upload.file_hash,
                mime_type: upload.mime_type,
                media_type: upload.media_type,
                blossom_url: upload.blossom_url,
                dimensions: upload.dimensions,
                blurhash: upload.blurhash,
            },
        )
        .await?;

        Ok(())
    }

    /// Gets a file path if it exists in cache
    ///
    /// # Arguments
    /// * `group_id` - The MLS group ID
    /// * `subdirectory` - Subdirectory within the group
    /// * `filename` - The filename to look for
    ///
    /// # Returns
    /// The path if the file exists, None otherwise
    #[allow(dead_code)]
    pub(crate) fn get_file_path(
        &self,
        group_id: &GroupId,
        subdirectory: &str,
        filename: &str,
    ) -> Option<PathBuf> {
        self.storage
            .media_files
            .get_file_path(group_id, subdirectory, filename)
    }

    /// Finds a file with a given prefix
    ///
    /// Useful when you know the hash but not the exact extension.
    ///
    /// # Arguments
    /// * `group_id` - The MLS group ID
    /// * `subdirectory` - Subdirectory within the group
    /// * `prefix` - The filename prefix to search for
    ///
    /// # Returns
    /// The path to the first matching file, if any
    pub(crate) fn find_file_with_prefix(
        &self,
        group_id: &GroupId,
        subdirectory: &str,
        prefix: &str,
    ) -> Option<PathBuf> {
        self.storage
            .media_files
            .find_file_with_prefix(group_id, subdirectory, prefix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_store_and_record() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Database::new(db_path).await.unwrap();
        let storage = Storage::new(temp_dir.path()).unwrap();

        let media_files = MediaFiles::new(&storage, &db);

        let group_id = GroupId::from_slice(&[1u8; 8]);
        let pubkey = PublicKey::from_slice(&[2u8; 32]).unwrap();
        let file_hash = [3u8; 32];
        let test_data = b"test file content";

        // Create test account to satisfy foreign key constraint
        sqlx::query("INSERT INTO users (pubkey, created_at, updated_at) VALUES (?, ?, ?)")
            .bind(pubkey.to_hex())
            .bind(chrono::Utc::now().timestamp())
            .bind(chrono::Utc::now().timestamp())
            .execute(&db.pool)
            .await
            .unwrap();

        let user_id: i64 = sqlx::query_scalar("SELECT id FROM users WHERE pubkey = ?")
            .bind(pubkey.to_hex())
            .fetch_one(&db.pool)
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO accounts (pubkey, user_id, created_at, updated_at) VALUES (?, ?, ?, ?)",
        )
        .bind(pubkey.to_hex())
        .bind(user_id)
        .bind(chrono::Utc::now().timestamp())
        .bind(chrono::Utc::now().timestamp())
        .execute(&db.pool)
        .await
        .unwrap();

        // Store and record
        let upload = MediaFileUpload {
            data: test_data,
            file_hash,
            mime_type: "image/jpeg",
            media_type: "test_media",
            blossom_url: None,
            dimensions: None,
            blurhash: None,
        };
        let path = media_files
            .store_and_record(&pubkey, &group_id, "test_subdir", "test.jpg", upload)
            .await
            .unwrap();

        // Verify file exists
        assert!(path.exists());

        // Verify content
        let content = tokio::fs::read(&path).await.unwrap();
        assert_eq!(content, test_data);

        // Verify database record
        use crate::whitenoise::database::media_files::MediaFile;
        let found = MediaFile::find_by_group_and_hash(&db, &group_id, &file_hash)
            .await
            .unwrap();

        assert!(found.is_some());
        let record = found.unwrap();
        assert_eq!(record.mime_type, "image/jpeg");
        assert_eq!(record.media_type, "test_media");
    }

    #[tokio::test]
    async fn test_find_file_with_prefix() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Database::new(db_path).await.unwrap();
        let storage = Storage::new(temp_dir.path()).unwrap();

        let media_files = MediaFiles::new(&storage, &db);

        let group_id = GroupId::from_slice(&[1u8; 8]);

        // Store files directly via storage
        storage
            .media_files
            .store_file(&group_id, "images", "abc123.jpg", b"jpeg data")
            .await
            .unwrap();

        // Find by prefix
        let found = media_files
            .find_file_with_prefix(&group_id, "images", "abc123")
            .unwrap();
        assert!(found.to_string_lossy().contains("abc123"));
    }
}
