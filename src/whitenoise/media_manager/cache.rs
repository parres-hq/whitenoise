//! This module contains functions for managing cached media files.

use std::path::Path;
use tokio::fs;

use anyhow::anyhow;
use nostr_mls::prelude::*;

use super::{
    errors::MediaError,
    types::{CachedMediaFile, MediaFile},
};
use crate::whitenoise::error::Result;
use crate::{Whitenoise, WhitenoiseError};

const MEDIA_CACHE_DIR: &str = "media_cache";

impl Whitenoise {
    /// Adds a file to the cache, saving it to disk and creating a database entry.
    ///
    /// # Arguments
    /// * `data` - The unencrypted file data to cache
    /// * `group_id` - The MLS group id that the media file belongs to
    /// * `account_pubkey` - Public Key of the account
    /// * `encrypted_file_hash` - Encrypted hash of the file stored in Blossom
    ///
    /// # Returns
    /// * `Ok(MediaFile) - Successfully return the newly added row
    /// * `Err(MediaError)` - Error if caching fails
    pub(crate) async fn add_to_cache(
        &self,
        data: &[u8],
        group_id: &GroupId,
        account_pubkey: &PublicKey,
        encrypted_file_hash: &[u8],
    ) -> Result<MediaFile> {
        let file_path = self.file_path_from_hash(group_id, encrypted_file_hash)?;
        // Ensure directory exists
        if let Some(parent) = Path::new(&file_path).parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| MediaError::Cache(e.to_string()))?;
        }

        // Write file to disk
        fs::write(&file_path, data)
            .await
            .map_err(|e| MediaError::Cache(e.to_string()))?;

        // Get current timestamp
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| MediaError::Cache(e.to_string()))?
            .as_secs();

        // Insert into database
        let media_file = sqlx::query_as::<_, MediaFile>(
            "
        INSERT INTO media_files (
            mls_group_id, account_pubkey,
            file_hash, created_at
        ) VALUES (?, ?, ?, ?) RETURNING *",
        )
        .bind(group_id.as_slice())
        .bind(account_pubkey.to_string())
        .bind(hex::encode(encrypted_file_hash))
        .bind(created_at as i64)
        .fetch_one(&self.database.pool)
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
    pub(crate) async fn fetch_cached_file(
        &self,
        group_id: &GroupId,
        file_hash: &[u8],
        whitenoise: &Whitenoise,
    ) -> Result<Option<CachedMediaFile>> {
        let media_file = sqlx::query_as::<_, MediaFile>(
            "SELECT * FROM media_files WHERE mls_group_id = ? AND file_hash = ?",
        )
        .bind(group_id.as_slice())
        .bind(hex::encode(file_hash))
        .fetch_optional(&whitenoise.database.pool)
        .await
        .map_err(|e| MediaError::Cache(e.to_string()))?;

        if let Some(media_file) = media_file {
            let file_path = whitenoise.file_path_from_hash(group_id, file_hash)?;
            let file_data = fs::read(file_path)
                .await
                .map_err(|e| MediaError::Cache(e.to_string()))?;
            Ok(Some(CachedMediaFile {
                media_file,
                file_data,
            }))
        } else {
            Ok(None)
        }
    }

    fn file_path_from_hash(&self, group_id: &GroupId, file_hash: &[u8]) -> Result<String> {
        // Create file path
        Ok(format!(
            "{}/{}/{}/{}",
            self.config
                .data_dir
                .to_str()
                .ok_or(WhitenoiseError::Other(anyhow!(
                    "Unable to convert PathBuf to string"
                )))?,
            MEDIA_CACHE_DIR,
            hex::encode(group_id.as_slice()),
            hex::encode(file_hash)
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::whitenoise::test_utils::create_mock_whitenoise;

    use super::*;

    #[tokio::test]
    async fn test_cache() {
        let data = b"Some test data";
        let group_id = GroupId::from_slice(b"securely generated 32 bytes in random");
        let encrypted_file_hash = b"32 byte hash of the encrypted data";
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let account = whitenoise.create_identity().await.unwrap();

        let media_file = whitenoise
            .add_to_cache(data, &group_id, &account.pubkey, encrypted_file_hash)
            .await
            .unwrap();

        // Fetch the cached data
        let some_cached_data = whitenoise
            .fetch_cached_file(&group_id, encrypted_file_hash, &whitenoise)
            .await
            .unwrap();
        let cached_data = some_cached_data.expect("Some data should be there");

        assert_eq!(cached_data.file_data, data);
        assert_eq!(media_file, cached_data.media_file);
    }
}
