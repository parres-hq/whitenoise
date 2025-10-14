use std::path::{Path, PathBuf};

use mdk_core::{
    GroupId, encrypted_media::types::EncryptedMediaUpload, extension::group_image::GroupImageUpload,
};
use nostr_sdk::PublicKey;

use crate::whitenoise::{
    database::{
        Database,
        media_files::{FileMetadata, MediaFile, MediaFileParams},
    },
    error::Result,
    storage::Storage,
};

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
    /// Optional file metadata (original filename, dimensions, blurhash, duration, etc.)
    pub file_metadata: Option<&'a FileMetadata>,
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
            file_metadata: None,
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
            file_metadata: None,
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
    /// 1. Stores the file to the filesystem (deduplicated by content)
    /// 2. Records the metadata in the database (linking this group to the file)
    ///
    /// Files with the same content (hash) are stored only once on disk.
    /// Multiple groups can reference the same file through database records.
    ///
    /// # Arguments
    /// * `account_pubkey` - The account accessing this file
    /// * `group_id` - The MLS group ID (for database relationship tracking)
    /// * `filename` - The filename to store as (typically `<hash>.<ext>`)
    /// * `upload` - MediaFileUpload containing file data and metadata
    ///
    /// # Returns
    /// The path to the stored file
    pub(crate) async fn store_and_record(
        &self,
        account_pubkey: &PublicKey,
        group_id: &GroupId,
        filename: &str,
        upload: MediaFileUpload<'_>,
    ) -> Result<PathBuf> {
        // Store file to filesystem (deduplicated by content)
        let file_path = self
            .storage
            .media_files
            .store_file(filename, upload.data)
            .await?;

        // Record in database (tracks group-file relationship)
        self.record_in_database(account_pubkey, group_id, &file_path, upload)
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
                file_metadata: upload.file_metadata,
            },
        )
        .await?;

        Ok(())
    }

    /// Finds a file with a given prefix
    ///
    /// Useful when you know the hash but not the exact extension.
    ///
    /// # Arguments
    /// * `prefix` - The filename prefix to search for
    ///
    /// # Returns
    /// The path to the first matching file, if any
    pub(crate) async fn find_file_with_prefix(&self, prefix: &str) -> Option<PathBuf> {
        self.storage.media_files.find_file_with_prefix(prefix).await
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
        let storage = Storage::new(temp_dir.path()).await.unwrap();

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
            file_metadata: None,
        };
        let path = media_files
            .store_and_record(&pubkey, &group_id, "test.jpg", upload)
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
        let storage = Storage::new(temp_dir.path()).await.unwrap();

        let media_files = MediaFiles::new(&storage, &db);

        // Store files directly via storage
        storage
            .media_files
            .store_file("abc123.jpg", b"jpeg data")
            .await
            .unwrap();

        // Find by prefix
        let found = media_files.find_file_with_prefix("abc123").await.unwrap();
        assert!(found.to_string_lossy().contains("abc123"));
    }
}
