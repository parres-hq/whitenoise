//! This module contains functions for managing cached media files.

use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use nostr_mls::prelude::*;
use sha2::{Digest, Sha256};

use super::sanitizer::SafeMediaMetadata;
use crate::database::Database;
use crate::media::errors::MediaError;
use crate::media::types::{CachedMediaFile, MediaFile};

const MEDIA_CACHE_DIR: &str = "media_cache";

/// Adds a file to the cache, saving it to disk and creating a database entry.
///
/// # Arguments
/// * `data` - The file data to cache
/// * `group` - The MLS group that the media file belongs to
/// * `blossom_url` - Optional URL of the file on Blossom
/// * `nostr_key` - Optional nostr key used for upload
/// * `file_metadata` - Optional JSON metadata for the file
/// * `data_dir` - The directory to save the file to
/// * `db` - Database connection
///
/// # Returns
/// * `Ok(MediaFile)` - The created media file
/// * `Err(MediaError)` - Error if caching fails
#[allow(clippy::too_many_arguments)]
pub async fn add_to_cache(
    data: &[u8],
    group: &group_types::Group,
    account_pubkey: &str,
    blossom_url: Option<String>,
    nostr_key: Option<String>,
    file_metadata: Option<SafeMediaMetadata>,
    data_dir: &str,
    db: &Database,
) -> Result<MediaFile, MediaError> {
    // Calculate file hash
    let mut hasher = Sha256::new();
    hasher.update(data);
    let file_hash = format!("{:x}", hasher.finalize());

    // Create file path
    let file_path = format!(
        "{}/{}/{}/{}",
        data_dir,
        MEDIA_CACHE_DIR,
        hex::encode(group.mls_group_id.as_slice()),
        file_hash
    );

    // Ensure directory exists
    if let Some(parent) = Path::new(&file_path).parent() {
        fs::create_dir_all(parent).map_err(|e| MediaError::Cache(e.to_string()))?;
    }

    // Write file to disk
    fs::write(&file_path, data).map_err(|e| MediaError::Cache(e.to_string()))?;

    // Get current timestamp
    let created_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| MediaError::Cache(e.to_string()))?
        .as_secs();

    // Insert into database
    let media_file = sqlx::query_as::<_, MediaFile>(
        "
        INSERT INTO media_files (
            mls_group_id, account_pubkey, file_path, blossom_url,
            file_hash, nostr_key, created_at, file_metadata
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        RETURNING *",
    )
    .bind(group.mls_group_id.as_slice())
    .bind(account_pubkey)
    .bind(&file_path)
    .bind(blossom_url)
    .bind(file_hash)
    .bind(nostr_key)
    .bind(created_at as i64)
    .bind(file_metadata.map(|m| serde_json::to_value(m).unwrap()))
    .fetch_one(&db.pool)
    .await?;

    Ok(media_file)
}

/// Fetches a cached file by its MLS group ID and file hash.
///
/// # Arguments
/// * `mls_group_id` - The MLS group ID
/// * `file_hash` - The hash of the file
/// * `db` - Database connection
///
/// # Returns
/// * `Ok(Option<CachedMediaFile>)` - The cached media file if found
/// * `Err(MediaError)` - Error if fetch fails or file not found
pub async fn fetch_cached_file(
    group: &group_types::Group,
    file_hash: &str,
    db: &Database,
) -> Result<Option<CachedMediaFile>, MediaError> {
    let media_file = sqlx::query_as::<_, MediaFile>(
        "SELECT * FROM media_files WHERE mls_group_id = ? AND file_hash = ?",
    )
    .bind(group.mls_group_id.as_slice())
    .bind(file_hash)
    .fetch_optional(&db.pool)
    .await
    .map_err(|e| MediaError::Cache(e.to_string()))?;

    if let Some(media_file) = media_file {
        let file_data =
            fs::read(media_file.file_path.clone()).map_err(|e| MediaError::Cache(e.to_string()))?;
        Ok(Some(CachedMediaFile {
            media_file,
            file_data,
        }))
    } else {
        Ok(None)
    }
}

/// Deletes a cached file from both disk and database.
///
/// # Arguments
/// * `mls_group_id` - The MLS group ID
/// * `file_hash` - The hash of the file
/// * `db` - Database connection
///
/// # Returns
/// * `Ok(())` - Success
/// * `Err(MediaError)` - Error if deletion fails
pub async fn delete_cached_file(
    group: &group_types::Group,
    file_hash: &str,
    db: &Database,
) -> Result<(), MediaError> {
    // First get the file path
    let cached_media_file = fetch_cached_file(group, file_hash, db).await?;

    if let Some(cached_media_file) = cached_media_file {
        // Delete from disk
        if Path::new(&cached_media_file.media_file.file_path).exists() {
            fs::remove_file(&cached_media_file.media_file.file_path)
                .map_err(|e| MediaError::Cache(e.to_string()))?;
        }

        // Delete from database (cascade will handle this)
        sqlx::query("DELETE FROM media_files WHERE mls_group_id = ? AND file_hash = ?")
            .bind(group.mls_group_id.as_slice())
            .bind(file_hash)
            .execute(&db.pool)
            .await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use sqlx::sqlite::SqlitePoolOptions;
    use tempfile::tempdir;

    use super::*;
    use crate::database::Database;

    async fn setup_test_db() -> (Database, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // Ensure the directory exists and is writable
        std::fs::create_dir_all(temp_dir.path()).expect("Failed to create temp directory");

        // Try to create an empty file first to test permissions
        std::fs::File::create(&db_path).expect("Failed to create database file");

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&format!("sqlite:{}", db_path.display()))
            .await
            .unwrap();

        // Create test table
        sqlx::query(
            "CREATE TABLE media_files (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                mls_group_id BLOB NOT NULL,
                account_pubkey TEXT NOT NULL,
                file_path TEXT NOT NULL,
                blossom_url TEXT,
                file_hash TEXT NOT NULL,
                nostr_key TEXT,
                created_at INTEGER NOT NULL,
                file_metadata JSONB
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        (
            Database {
                pool,
                path: db_path,
                last_connected: SystemTime::now(),
            },
            temp_dir,
        )
    }

    #[tokio::test]
    async fn test_add_and_fetch_cache() {
        let (db, temp_dir) = setup_test_db().await;
        let test_data = b"test file content";
        let group = group_types::Group {
            mls_group_id: GroupId::from_slice(&[1, 2, 3]),
            nostr_group_id: [0u8; 32],
            name: "test_group".to_string(),
            description: "test_description".to_string(),
            admin_pubkeys: BTreeSet::new(),
            last_message_id: None,
            last_message_at: None,
            epoch: 0,
            state: group_types::GroupState::Active,
        };
        let data_dir = temp_dir.path().to_str().unwrap();
        let account_pubkey = "test_pubkey";
        // Calculate expected file hash
        let mut hasher = Sha256::new();
        hasher.update(test_data);
        let expected_hash = format!("{:x}", hasher.finalize());

        // Add file to cache with metadata
        let metadata = SafeMediaMetadata {
            mime_type: "text/plain".to_string(),
            size_bytes: test_data.len() as u64,
            format: Some("txt".to_string()),
            dimensions: None,
            color_space: None,
            has_alpha: None,
            bits_per_pixel: None,
            duration_seconds: None,
            frame_rate: None,
            video_codec: None,
            audio_codec: None,
            video_bitrate: None,
            audio_bitrate: None,
            video_dimensions: None,
            page_count: None,
            author: None,
            title: None,
            created_at: None,
            modified_at: None,
        };

        let media_file = add_to_cache(
            test_data,
            &group,
            account_pubkey,
            Some("https://example.com/test.txt".to_string()),
            Some("nostr_key".to_string()),
            Some(metadata),
            data_dir,
            &db,
        )
        .await
        .unwrap();

        // Verify database entry
        assert_eq!(media_file.mls_group_id, group.mls_group_id);
        assert_eq!(media_file.file_hash, expected_hash);
        assert_eq!(
            media_file.blossom_url,
            Some("https://example.com/test.txt".to_string())
        );
        assert_eq!(media_file.nostr_key, Some("nostr_key".to_string()));
        assert!(media_file.file_metadata.is_some());
        if let Some(metadata) = media_file.file_metadata {
            assert_eq!(metadata.mime_type, "text/plain");
            assert_eq!(metadata.size_bytes, test_data.len() as u64);
            assert_eq!(metadata.format, Some("txt".to_string()));
        }

        // Verify file path structure
        let expected_path = format!(
            "{}/{}/{}/{}",
            data_dir,
            MEDIA_CACHE_DIR,
            hex::encode(group.mls_group_id.as_slice()),
            expected_hash
        );
        assert_eq!(media_file.file_path, expected_path);

        // Fetch file
        let fetched = fetch_cached_file(&group, &expected_hash, &db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fetched.media_file.id, media_file.id);
        assert_eq!(fetched.media_file.file_path, expected_path);
        assert_eq!(fetched.file_data, test_data);

        // Verify file exists on disk
        assert!(Path::new(&expected_path).exists());
        let contents = fs::read_to_string(&expected_path).unwrap();
        assert_eq!(contents, "test file content");
    }

    #[tokio::test]
    async fn test_delete_cache() {
        let (db, temp_dir) = setup_test_db().await;
        let test_data = b"test file content";
        let group = group_types::Group {
            mls_group_id: GroupId::from_slice(&[1, 2, 3]),
            nostr_group_id: [0u8; 32],
            name: "test_group".to_string(),
            description: "test_description".to_string(),
            admin_pubkeys: BTreeSet::new(),
            last_message_id: None,
            last_message_at: None,
            epoch: 0,
            state: group_types::GroupState::Active,
        };
        let data_dir = temp_dir.path().to_str().unwrap();

        // Calculate expected file hash
        let mut hasher = Sha256::new();
        hasher.update(test_data);
        let expected_hash = format!("{:x}", hasher.finalize());

        // Add file to cache
        let _media_file = add_to_cache(
            test_data,
            &group,
            "test_pubkey",
            None,
            None,
            None,
            data_dir,
            &db,
        )
        .await
        .unwrap();

        // Verify file exists
        let expected_path = format!(
            "{}/{}/{}/{}",
            data_dir,
            MEDIA_CACHE_DIR,
            hex::encode(group.mls_group_id.as_slice()),
            expected_hash
        );
        assert!(Path::new(&expected_path).exists());
        assert!(fetch_cached_file(&group, &expected_hash, &db)
            .await
            .unwrap()
            .is_some());

        // Delete file
        delete_cached_file(&group, &expected_hash, &db)
            .await
            .unwrap();

        // Verify file is deleted
        assert!(!Path::new(&expected_path).exists());
        assert!(fetch_cached_file(&group, &expected_hash, &db)
            .await
            .unwrap()
            .is_none());
    }
}
