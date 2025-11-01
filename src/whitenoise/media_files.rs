use std::path::{Path, PathBuf};

use mdk_core::GroupId;
use nostr_sdk::{PublicKey, prelude::*};

pub use crate::whitenoise::database::media_files::MediaFile;
use crate::whitenoise::{
    database::{
        Database,
        media_files::{FileMetadata, MediaFileParams},
    },
    error::{Result, WhitenoiseError},
    storage::Storage,
};

/// Parsed imeta tag following MIP-04 specification
///
/// MIP-04 specifies the format for media metadata in Nostr events:
/// ["imeta", "url <blossom_url>", "x <original_hash>", "m <mime_type>", ...]
///
/// See: https://github.com/parres-hq/marmot/blob/master/04.md
#[derive(Debug, Clone, PartialEq, Eq)]
struct ImetaTag {
    /// Blossom URL where the encrypted media is stored
    url: String,
    /// Original file hash (hex-encoded SHA-256 of decrypted content)
    /// This is the value in the 'x' field per MIP-04
    x_field: String,
    /// MIME type of the file
    mime_type: String,
    /// Original filename (optional)
    filename: Option<String>,
    /// Image dimensions as "widthxheight" (optional)
    dimensions: Option<String>,
    /// Blurhash string for image preview (optional)
    blurhash: Option<String>,
}

/// Extracts imeta tags from Nostr event tags (MIP-04 format)
///
/// Parses tags with the format:
/// ["imeta", "url <blossom_url>", "x <original_hash>", "m <mime>", "name <filename>", ...]
///
/// Returns an empty vector if no valid imeta tags are found.
fn extract_imeta_tags(tags: &Tags) -> Vec<ImetaTag> {
    let mut imeta_tags = Vec::new();

    for tag in tags.iter() {
        if tag.kind() != TagKind::Custom("imeta".into()) {
            continue;
        }

        // Parse imeta tag parameters
        let tag_vec = tag.clone().to_vec();
        let mut url = None;
        let mut x_field = None;
        let mut mime_type = None;
        let mut filename = None;
        let mut dimensions = None;
        let mut blurhash = None;

        // Skip first element (tag name "imeta") and parse remaining key-value pairs
        for value in tag_vec.iter().skip(1) {
            if let Some(blossom_url) = value.strip_prefix("url ") {
                url = Some(blossom_url.to_string());
            } else if let Some(hash) = value.strip_prefix("x ") {
                x_field = Some(hash.to_string());
            } else if let Some(mime) = value.strip_prefix("m ") {
                mime_type = Some(mime.to_string());
            } else if let Some(name) = value.strip_prefix("name ") {
                filename = Some(name.to_string());
            } else if let Some(dim) = value.strip_prefix("dim ") {
                dimensions = Some(dim.to_string());
            } else if let Some(blur) = value.strip_prefix("blurhash ") {
                blurhash = Some(blur.to_string());
            }
        }

        // Only include if required fields are present
        if let (Some(url), Some(x_field), Some(mime_type)) = (url, x_field, mime_type) {
            imeta_tags.push(ImetaTag {
                url,
                x_field,
                mime_type,
                filename,
                dimensions,
                blurhash,
            });
        }
    }

    imeta_tags
}

/// Extracts encrypted hash from Blossom URL
///
/// Blossom URL format: https://blossom.server/<hash>
/// The hash is the encrypted_file_hash (SHA-256 of encrypted blob)
///
/// # Returns
/// * `Ok([u8; 32])` - The encrypted file hash
/// * `Err(WhitenoiseError)` - If URL is malformed or hash is invalid
fn extract_hash_from_blossom_url(url: &str) -> Result<[u8; 32]> {
    let parsed_url = Url::parse(url).map_err(|e| {
        WhitenoiseError::InvalidInput(format!("Invalid Blossom URL '{}': {}", url, e))
    })?;

    let hash_hex = parsed_url
        .path()
        .trim_start_matches('/')
        .trim_end_matches('/');

    if hash_hex.is_empty() {
        return Err(WhitenoiseError::InvalidInput(
            "Blossom URL contains no hash".to_string(),
        ));
    }

    let hash_bytes = hex::decode(hash_hex).map_err(|e| {
        WhitenoiseError::InvalidInput(format!(
            "Invalid hex in Blossom URL hash '{}': {}",
            hash_hex, e
        ))
    })?;

    let hash_len = hash_bytes.len();
    hash_bytes.try_into().map_err(|_| {
        WhitenoiseError::InvalidInput(format!(
            "Invalid hash length in Blossom URL: expected 32 bytes, got {}",
            hash_len
        ))
    })
}

/// Intermediate type for media file storage operations
///
/// This type abstracts over different MDK upload types (GroupImageUpload, EncryptedMediaUpload)
/// and provides a unified interface for storing media files.
pub(crate) struct MediaFileUpload<'a> {
    /// The decrypted file data to store
    pub data: &'a [u8],
    /// SHA-256 hash of the original/decrypted content (for MIP-04 x field, MDK key derivation)
    /// None for group images (which use key/nonce encryption), Some for chat media (MDK)
    pub original_file_hash: Option<&'a [u8; 32]>,
    /// SHA-256 hash of the encrypted file (for Blossom verification)
    pub encrypted_file_hash: [u8; 32],
    /// MIME type of the file
    pub mime_type: &'a str,
    /// Type of media (e.g., "group_image", "chat_media")
    pub media_type: &'a str,
    /// Optional Blossom URL where the encrypted file is stored
    pub blossom_url: Option<&'a str>,
    /// Optional Nostr key (hex-encoded secret key) used for upload authentication/cleanup
    /// For group images: deterministically derived from image_key (stored for convenience)
    /// For chat images: randomly generated per upload (must be stored)
    pub nostr_key: Option<String>,
    /// Optional file metadata (original filename, dimensions, blurhash, duration, etc.)
    pub file_metadata: Option<&'a FileMetadata>,
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
    /// The MediaFile record from the database
    pub(crate) async fn store_and_record(
        &self,
        account_pubkey: &PublicKey,
        group_id: &GroupId,
        filename: &str,
        upload: MediaFileUpload<'_>,
    ) -> Result<MediaFile> {
        // Store file to filesystem (deduplicated by content)
        let file_path = self
            .storage
            .media_files
            .store_file(filename, upload.data)
            .await?;

        // Record in database (tracks group-file relationship) and return the MediaFile
        self.record_in_database(account_pubkey, group_id, &file_path, upload)
            .await
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
    ///
    /// # Returns
    /// The MediaFile record from the database
    pub(crate) async fn record_in_database(
        &self,
        account_pubkey: &PublicKey,
        group_id: &GroupId,
        file_path: &Path,
        upload: MediaFileUpload<'_>,
    ) -> Result<MediaFile> {
        let media_file = MediaFile::save(
            self.database,
            group_id,
            account_pubkey,
            MediaFileParams {
                file_path,
                original_file_hash: upload.original_file_hash,
                encrypted_file_hash: &upload.encrypted_file_hash,
                mime_type: upload.mime_type,
                media_type: upload.media_type,
                blossom_url: upload.blossom_url,
                nostr_key: upload.nostr_key.as_deref(),
                file_metadata: upload.file_metadata,
            },
        )
        .await?;

        Ok(media_file)
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

    /// Stores media references from imeta tags into the database
    ///
    /// Creates MediaFile records without downloading the actual files.
    /// The file_path will be empty until download_chat_media() is called.
    ///
    /// This method is called synchronously during message processing to ensure
    /// MediaFile records exist before the message aggregator runs.
    ///
    /// # Error Handling
    /// Individual malformed imeta tags are logged and skipped rather than failing
    /// the entire message. This ensures one bad attachment doesn't break message delivery.
    ///
    /// # Arguments
    /// * `group_id` - The MLS group ID
    /// * `account_pubkey` - The account receiving the message
    /// * `inner_event` - The decrypted inner event (UnsignedEvent) containing imeta tags
    ///
    /// # Returns
    /// * `Ok(())` - All valid imeta tags processed (malformed ones logged and skipped)
    /// * `Err(WhitenoiseError)` - Database error or other system failure
    pub(crate) async fn store_references_from_imeta_tags(
        &self,
        group_id: &GroupId,
        account_pubkey: &PublicKey,
        inner_event: &UnsignedEvent,
    ) -> Result<()> {
        let imeta_tags = extract_imeta_tags(&inner_event.tags);

        if imeta_tags.is_empty() {
            return Ok(());
        }

        tracing::debug!(
            target: "whitenoise::store_media_references",
            "Found {} imeta tags in message",
            imeta_tags.len()
        );

        for imeta in imeta_tags {
            // Parse original_file_hash from x field (MIP-04 compliant)
            let original_file_hash = match hex::decode(&imeta.x_field) {
                Ok(bytes) => {
                    let bytes_len = bytes.len();
                    match bytes.try_into() {
                        Ok(hash) => hash,
                        Err(_) => {
                            tracing::warn!(
                                target: "whitenoise::store_media_references",
                                "Skipping malformed imeta tag: invalid hash length in x field (expected 32 bytes, got {})",
                                bytes_len
                            );
                            continue;
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        target: "whitenoise::store_media_references",
                        "Skipping malformed imeta tag: invalid hex in x field: {}",
                        e
                    );
                    continue;
                }
            };

            // Extract encrypted_file_hash from Blossom URL (REQUIRED - NOT NULL in DB)
            let encrypted_file_hash = match extract_hash_from_blossom_url(&imeta.url) {
                Ok(hash) => hash,
                Err(e) => {
                    tracing::warn!(
                        target: "whitenoise::store_media_references",
                        "Skipping malformed imeta tag: failed to extract encrypted hash from Blossom URL '{}': {}",
                        imeta.url,
                        e
                    );
                    continue;
                }
            };

            // Parse dimensions if present
            let dimensions = imeta.dimensions.as_ref().and_then(|d| {
                let parts: Vec<&str> = d.split('x').collect();
                if parts.len() == 2 {
                    Some(format!("{}x{}", parts[0], parts[1]))
                } else {
                    None
                }
            });

            // Create file metadata if any metadata fields are present
            let file_metadata =
                if imeta.filename.is_some() || dimensions.is_some() || imeta.blurhash.is_some() {
                    Some(FileMetadata {
                        original_filename: imeta.filename.clone(),
                        dimensions,
                        blurhash: imeta.blurhash.clone(),
                    })
                } else {
                    None
                };

            // Create MediaFile record (without file yet - empty path until downloaded)
            MediaFile::save(
                self.database,
                group_id,
                account_pubkey,
                MediaFileParams {
                    file_path: &PathBuf::from(""), // Empty until downloaded
                    original_file_hash: Some(&original_file_hash),
                    encrypted_file_hash: &encrypted_file_hash,
                    mime_type: &imeta.mime_type,
                    media_type: "chat_media",
                    blossom_url: Some(&imeta.url),
                    nostr_key: None, // Chat media uses MDK, not key/nonce
                    file_metadata: file_metadata.as_ref(),
                },
            )
            .await?;

            tracing::debug!(
                target: "whitenoise::store_media_references",
                "Stored media reference: original_hash={}, encrypted_hash={}, mime_type={}",
                hex::encode(original_file_hash),
                hex::encode(encrypted_file_hash),
                imeta.mime_type
            );
        }

        Ok(())
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
        let encrypted_file_hash = [3u8; 32];
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
            original_file_hash: None,
            encrypted_file_hash,
            mime_type: "image/jpeg",
            media_type: "test_media",
            blossom_url: None,
            nostr_key: None,
            file_metadata: None,
        };
        let media_file = media_files
            .store_and_record(&pubkey, &group_id, "test.jpg", upload)
            .await
            .unwrap();

        // Verify file exists on disk
        assert!(media_file.file_path.exists());

        // Verify file content is correct
        let content = tokio::fs::read(&media_file.file_path).await.unwrap();
        assert_eq!(content, test_data);

        // Verify idempotency: calling store_and_record again should succeed
        let upload2 = MediaFileUpload {
            data: test_data,
            original_file_hash: None,
            encrypted_file_hash,
            mime_type: "image/jpeg",
            media_type: "test_media",
            blossom_url: None,
            nostr_key: None,
            file_metadata: None,
        };
        let media_file2 = media_files
            .store_and_record(&pubkey, &group_id, "test.jpg", upload2)
            .await
            .unwrap();

        // Should return the same path
        assert_eq!(media_file.file_path, media_file2.file_path);
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

    #[test]
    fn test_extract_imeta_tags_valid() {
        let mut tags = Tags::new();
        tags.push(
            Tag::parse(vec![
                "imeta",
                "url https://blossom.example.com/abc123",
                "x 64characterhexhash0123456789abcdef0123456789abcdef0123456789abcdef",
                "m image/png",
                "name test.png",
                "dim 1920x1080",
                "blurhash LKO2?U%2Tw=w]~RBVZRi};RPxuwH",
            ])
            .unwrap(),
        );

        let imeta_tags = extract_imeta_tags(&tags);

        assert_eq!(imeta_tags.len(), 1);
        assert_eq!(imeta_tags[0].url, "https://blossom.example.com/abc123");
        assert_eq!(
            imeta_tags[0].x_field,
            "64characterhexhash0123456789abcdef0123456789abcdef0123456789abcdef"
        );
        assert_eq!(imeta_tags[0].mime_type, "image/png");
        assert_eq!(imeta_tags[0].filename, Some("test.png".to_string()));
        assert_eq!(imeta_tags[0].dimensions, Some("1920x1080".to_string()));
        assert_eq!(
            imeta_tags[0].blurhash,
            Some("LKO2?U%2Tw=w]~RBVZRi};RPxuwH".to_string())
        );
    }

    #[test]
    fn test_extract_imeta_tags_minimal() {
        let mut tags = Tags::new();
        tags.push(
            Tag::parse(vec![
                "imeta",
                "url https://blossom.example.com/hash",
                "x abc123",
                "m video/mp4",
            ])
            .unwrap(),
        );

        let imeta_tags = extract_imeta_tags(&tags);

        assert_eq!(imeta_tags.len(), 1);
        assert_eq!(imeta_tags[0].url, "https://blossom.example.com/hash");
        assert_eq!(imeta_tags[0].x_field, "abc123");
        assert_eq!(imeta_tags[0].mime_type, "video/mp4");
        assert_eq!(imeta_tags[0].filename, None);
        assert_eq!(imeta_tags[0].dimensions, None);
        assert_eq!(imeta_tags[0].blurhash, None);
    }

    #[test]
    fn test_extract_imeta_tags_missing_required_fields() {
        // Missing URL
        let mut tags = Tags::new();
        tags.push(Tag::parse(vec!["imeta", "x abc123", "m image/png"]).unwrap());
        assert_eq!(extract_imeta_tags(&tags).len(), 0);

        // Missing x field
        let mut tags = Tags::new();
        tags.push(
            Tag::parse(vec!["imeta", "url https://example.com/hash", "m image/png"]).unwrap(),
        );
        assert_eq!(extract_imeta_tags(&tags).len(), 0);

        // Missing mime type
        let mut tags = Tags::new();
        tags.push(Tag::parse(vec!["imeta", "url https://example.com/hash", "x abc123"]).unwrap());
        assert_eq!(extract_imeta_tags(&tags).len(), 0);
    }

    #[test]
    fn test_extract_imeta_tags_empty() {
        let tags = Tags::new();
        assert_eq!(extract_imeta_tags(&tags).len(), 0);
    }

    #[test]
    fn test_extract_imeta_tags_multiple() {
        let mut tags = Tags::new();
        tags.push(
            Tag::parse(vec![
                "imeta",
                "url https://example.com/hash1",
                "x hash1",
                "m image/png",
            ])
            .unwrap(),
        );
        tags.push(
            Tag::parse(vec![
                "imeta",
                "url https://example.com/hash2",
                "x hash2",
                "m video/mp4",
            ])
            .unwrap(),
        );

        let imeta_tags = extract_imeta_tags(&tags);

        assert_eq!(imeta_tags.len(), 2);
        assert_eq!(imeta_tags[0].x_field, "hash1");
        assert_eq!(imeta_tags[1].x_field, "hash2");
    }

    #[test]
    fn test_extract_hash_from_blossom_url_valid() {
        let url = "https://blossom.example.com/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let result = extract_hash_from_blossom_url(url);
        assert!(result.is_ok());
        let hash = result.unwrap();
        assert_eq!(hash.len(), 32);
        assert_eq!(
            hex::encode(hash),
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        );
    }

    #[test]
    fn test_extract_hash_from_blossom_url_with_trailing_slash() {
        let url = "https://blossom.example.com/abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234/";
        let result = extract_hash_from_blossom_url(url);
        assert!(result.is_ok());
    }

    #[test]
    fn test_extract_hash_from_blossom_url_invalid_hex() {
        let url = "https://blossom.example.com/notahexstring";
        let result = extract_hash_from_blossom_url(url);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid hex"));
    }

    #[test]
    fn test_extract_hash_from_blossom_url_wrong_length() {
        let url = "https://blossom.example.com/abc123"; // Too short
        let result = extract_hash_from_blossom_url(url);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid hash length")
        );
    }

    #[test]
    fn test_extract_hash_from_blossom_url_no_hash() {
        let url = "https://blossom.example.com/";
        let result = extract_hash_from_blossom_url(url);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("contains no hash"));
    }

    #[test]
    fn test_extract_hash_from_blossom_url_invalid_url() {
        let url = "not-a-url";
        let result = extract_hash_from_blossom_url(url);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid Blossom URL")
        );
    }
}
