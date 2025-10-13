use chrono::{DateTime, Utc};
use mdk_core::GroupId;
use nostr_sdk::PublicKey;
use serde::{Deserialize, Serialize};
use sqlx::types::Json;
use std::path::{Path, PathBuf};

use super::{Database, DatabaseError, utils::parse_timestamp};
use crate::whitenoise::error::WhitenoiseError;

/// Optional metadata for media files stored as JSONB
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct FileMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_filename: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub blurhash: Option<String>,
}

impl FileMetadata {
    /// Creates a new FileMetadata with all fields set to None
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_filename(mut self, original_filename: String) -> Self {
        self.original_filename = Some(original_filename);
        self
    }

    pub fn with_dimensions(mut self, dimensions: String) -> Self {
        self.dimensions = Some(dimensions);
        self
    }

    pub fn with_blurhash(mut self, blurhash: String) -> Self {
        self.blurhash = Some(blurhash);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.original_filename.is_none() && self.dimensions.is_none() && self.blurhash.is_none()
    }
}

/// Internal database row representation for media_files table
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct MediaFileRow {
    pub id: i64,
    pub mls_group_id: GroupId,
    pub account_pubkey: PublicKey,
    pub file_path: PathBuf,
    pub file_hash: Vec<u8>,
    pub mime_type: String,
    pub media_type: String,
    pub blossom_url: Option<String>,
    pub nostr_key: Option<String>,
    pub file_metadata: Option<FileMetadata>,
    pub created_at: DateTime<Utc>,
}

impl<'r, R> sqlx::FromRow<'r, R> for MediaFileRow
where
    R: sqlx::Row,
    &'r str: sqlx::ColumnIndex<R>,
    String: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
    i64: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
    Vec<u8>: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
{
    fn from_row(row: &'r R) -> std::result::Result<Self, sqlx::Error> {
        let id: i64 = row.try_get("id")?;
        let mls_group_id_bytes: Vec<u8> = row.try_get("mls_group_id")?;
        let account_pubkey_str: String = row.try_get("account_pubkey")?;
        let file_path_str: String = row.try_get("file_path")?;
        let file_hash_hex: String = row.try_get("file_hash")?;

        // Parse MLS group ID
        let mls_group_id = GroupId::from_slice(&mls_group_id_bytes);

        // Parse account pubkey
        let account_pubkey =
            PublicKey::parse(&account_pubkey_str).map_err(|e| sqlx::Error::ColumnDecode {
                index: "account_pubkey".to_string(),
                source: Box::new(e),
            })?;

        // Parse file path
        let file_path = PathBuf::from(file_path_str);

        // Parse file hash from hex
        let file_hash = hex::decode(file_hash_hex).map_err(|e| sqlx::Error::ColumnDecode {
            index: "file_hash".to_string(),
            source: Box::new(e),
        })?;

        let mime_type: String = row.try_get("mime_type")?;
        let media_type: String = row.try_get("media_type")?;
        let blossom_url: Option<String> = row.try_get("blossom_url")?;
        let nostr_key: Option<String> = row.try_get("nostr_key")?;

        // Deserialize file_metadata from JSON stored as TEXT/BLOB
        // We can't use Json<T> directly here because our generic FromRow implementation
        // doesn't constrain the database type, so we deserialize manually
        let file_metadata: Option<FileMetadata> = row
            .try_get::<Option<String>, _>("file_metadata")?
            .and_then(|json_str| serde_json::from_str(&json_str).ok());

        let created_at = parse_timestamp(row, "created_at")?;

        Ok(Self {
            id,
            mls_group_id,
            account_pubkey,
            file_path,
            file_hash,
            mime_type,
            media_type,
            blossom_url,
            nostr_key,
            file_metadata,
            created_at,
        })
    }
}

/// Parameters for saving a media file
#[derive(Debug, Clone)]
pub struct MediaFileParams<'a> {
    pub file_path: &'a Path,
    pub file_hash: &'a [u8; 32],
    pub mime_type: &'a str,
    pub media_type: &'a str,
    pub blossom_url: Option<&'a str>,
    pub file_metadata: Option<&'a FileMetadata>,
}

/// Represents a cached media file
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaFile {
    pub id: Option<i64>,
    pub mls_group_id: GroupId,
    pub account_pubkey: PublicKey,
    pub file_path: PathBuf,
    pub file_hash: Vec<u8>,
    pub mime_type: String,
    pub media_type: String,
    pub blossom_url: Option<String>,
    pub nostr_key: Option<String>,
    pub file_metadata: Option<FileMetadata>,
    pub created_at: DateTime<Utc>,
}

impl From<MediaFileRow> for MediaFile {
    fn from(val: MediaFileRow) -> Self {
        Self {
            id: Some(val.id),
            mls_group_id: val.mls_group_id,
            account_pubkey: val.account_pubkey,
            file_path: val.file_path,
            file_hash: val.file_hash,
            mime_type: val.mime_type,
            media_type: val.media_type,
            blossom_url: val.blossom_url,
            nostr_key: val.nostr_key,
            file_metadata: val.file_metadata,
            created_at: val.created_at,
        }
    }
}

impl MediaFile {
    /// Saves a cached media file to the database
    ///
    /// Inserts a new row or ignores if the record already exists
    /// (based on unique constraint on mls_group_id, file_hash, account_pubkey)
    ///
    /// # Arguments
    /// * `database` - The database connection
    /// * `mls_group_id` - The MLS group ID
    /// * `account_pubkey` - The account public key accessing this media
    /// * `params` - Media file parameters (path, hash, mime type, etc.)
    ///
    /// # Returns
    /// The MediaFile with the database-assigned ID
    ///
    /// # Errors
    /// Returns a [`WhitenoiseError`] if the database operation fails.
    pub(crate) async fn save(
        database: &Database,
        mls_group_id: &GroupId,
        account_pubkey: &PublicKey,
        params: MediaFileParams<'_>,
    ) -> Result<Self, WhitenoiseError> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let file_hash_hex = hex::encode(params.file_hash);
        let file_path_str = params
            .file_path
            .to_str()
            .ok_or_else(|| WhitenoiseError::MediaCache("Invalid file path".to_string()))?;

        // Wrap file_metadata in Json for automatic serialization
        // Only store if not empty (optimization)
        let file_metadata_json = params.file_metadata.filter(|m| !m.is_empty()).map(Json);

        let row = sqlx::query_as::<_, MediaFileRow>(
            "INSERT INTO media_files (
                mls_group_id, account_pubkey, file_path, file_hash,
                mime_type, media_type, blossom_url, nostr_key,
                file_metadata, created_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, NULL, ?, ?)
            ON CONFLICT (mls_group_id, file_hash, account_pubkey)
            DO NOTHING
            RETURNING id, mls_group_id, account_pubkey, file_path, file_hash,
                      mime_type, media_type, blossom_url, nostr_key,
                      file_metadata, created_at",
        )
        .bind(mls_group_id.as_slice())
        .bind(account_pubkey.to_hex())
        .bind(file_path_str)
        .bind(file_hash_hex)
        .bind(params.mime_type)
        .bind(params.media_type)
        .bind(params.blossom_url)
        .bind(file_metadata_json)
        .bind(now_ms)
        .fetch_one(&database.pool)
        .await
        .map_err(DatabaseError::Sqlx)?;

        Ok(row.into())
    }

    /// Finds a cached media file by group and hash
    ///
    /// Returns the first matching record (any account) if it exists in cache
    ///
    /// # Arguments
    /// * `database` - The database connection
    /// * `mls_group_id` - The MLS group ID
    /// * `file_hash` - The hash of the ENCRYPTED data (32 bytes)
    ///
    /// # Returns
    /// The media file if found, None otherwise
    ///
    /// # Errors
    /// Returns a [`WhitenoiseError`] if the database query fails.
    #[allow(unused)]
    pub(crate) async fn find_by_group_and_hash(
        database: &Database,
        mls_group_id: &GroupId,
        file_hash: &[u8; 32],
    ) -> Result<Option<Self>, WhitenoiseError> {
        let file_hash_hex = hex::encode(file_hash);

        let result = sqlx::query_as::<_, MediaFileRow>(
            "SELECT id, mls_group_id, account_pubkey, file_path, file_hash,
                    mime_type, media_type, blossom_url, nostr_key,
                    file_metadata, created_at
             FROM media_files
             WHERE mls_group_id = ? AND file_hash = ?
             LIMIT 1",
        )
        .bind(mls_group_id.as_slice())
        .bind(file_hash_hex)
        .fetch_optional(&database.pool)
        .await
        .map_err(DatabaseError::Sqlx)?;

        Ok(result.map(Self::from))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_account(db: &Database, pubkey: &PublicKey) {
        // Create test user and account to satisfy foreign key constraints
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
    }

    #[tokio::test]
    async fn test_record_and_find_cached_media() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Database::new(db_path).await.unwrap();

        // Create a test group ID
        let group_id = mdk_core::GroupId::from_slice(&[1u8; 8]);
        let pubkey = PublicKey::from_slice(&[2u8; 32]).unwrap();
        let file_hash = [3u8; 32];
        let file_path = temp_dir.path().join("test.jpg");

        // Create test account to satisfy foreign key constraint
        create_test_account(&db, &pubkey).await;

        // Save media
        let media_file = MediaFile::save(
            &db,
            &group_id,
            &pubkey,
            MediaFileParams {
                file_path: &file_path,
                file_hash: &file_hash,
                mime_type: "image/jpeg",
                media_type: "group_image",
                blossom_url: None,
                file_metadata: None,
            },
        )
        .await
        .unwrap();

        assert!(media_file.id.is_some());
        assert!(media_file.id.unwrap() > 0);

        // Find media
        let found = MediaFile::find_by_group_and_hash(&db, &group_id, &file_hash)
            .await
            .unwrap();

        assert!(found.is_some());
        let record = found.unwrap();
        assert_eq!(record.id, media_file.id);
        assert_eq!(record.file_hash, file_hash.to_vec());
        assert_eq!(record.mime_type, "image/jpeg");
        assert_eq!(record.media_type, "group_image");
    }
}
