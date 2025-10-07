use crate::whitenoise::error::{Result, WhitenoiseError};
use mdk_core::GroupId;
use std::path::{Path, PathBuf};
use tokio::fs;

/// Filesystem storage for media files organized by MLS group
///
/// Directory structure:
/// ```
/// <cache_dir>/
///   <mls_group_id_hex>/
///     <subdirectory>/
///       <filename>
/// ```
///
/// This module handles:
/// - Creating directory structures
/// - Atomic file writes
/// - File retrieval by path
/// - File existence checks
/// - Finding files by prefix
///
/// It does NOT handle:
/// - Database operations (see database::media_files)
/// - Network operations (see BlossomClient)
/// - Encryption/decryption (caller's responsibility)
pub struct MediaFileStorage {
    cache_dir: PathBuf,
}

impl MediaFileStorage {
    /// Creates a new MediaFileStorage instance
    ///
    /// # Arguments
    /// * `data_dir` - The application data directory
    ///
    /// # Returns
    /// A new MediaFileStorage instance with cache directory at `<data_dir>/media_cache/`
    pub(crate) fn new(data_dir: &Path) -> Result<Self> {
        let cache_dir = data_dir.join("media_cache");

        // Create cache directory if it doesn't exist
        if !cache_dir.exists() {
            std::fs::create_dir_all(&cache_dir).map_err(|e| {
                WhitenoiseError::MediaCache(format!("Failed to create cache directory: {}", e))
            })?;
        }

        Ok(Self { cache_dir })
    }

    /// Stores a file in the cache using atomic write
    ///
    /// # Arguments
    /// * `group_id` - The MLS group ID
    /// * `subdirectory` - Subdirectory within the group (e.g., "group_images", "media")
    /// * `filename` - The filename to store as (e.g., "abc123.jpg")
    /// * `data` - The file data to store
    ///
    /// # Returns
    /// The full path to the cached file
    ///
    /// # Errors
    /// Returns error if filesystem operations fail
    pub(crate) async fn store_file(
        &self,
        group_id: &GroupId,
        subdirectory: &str,
        filename: &str,
        data: &[u8],
    ) -> Result<PathBuf> {
        let group_dir = self.get_group_dir(group_id, subdirectory);

        // Create directory if it doesn't exist
        if !group_dir.exists() {
            fs::create_dir_all(&group_dir).await.map_err(|e| {
                WhitenoiseError::MediaCache(format!("Failed to create cache directory: {}", e))
            })?;
        }

        let file_path = group_dir.join(filename);

        // Write atomically: write to temp file, then rename
        let temp_path = file_path.with_extension("tmp");

        fs::write(&temp_path, data).await.map_err(|e| {
            WhitenoiseError::MediaCache(format!("Failed to write file data: {}", e))
        })?;

        fs::rename(&temp_path, &file_path).await.map_err(|e| {
            WhitenoiseError::MediaCache(format!("Failed to rename temp file: {}", e))
        })?;

        Ok(file_path)
    }

    /// Gets the path to a cached file if it exists
    ///
    /// # Arguments
    /// * `group_id` - The MLS group ID
    /// * `subdirectory` - Subdirectory within the group
    /// * `filename` - The filename to look for
    ///
    /// # Returns
    /// The path if the file exists, None otherwise
    pub(crate) fn get_file_path(
        &self,
        group_id: &GroupId,
        subdirectory: &str,
        filename: &str,
    ) -> Option<PathBuf> {
        let file_path = self.get_group_dir(group_id, subdirectory).join(filename);

        if file_path.exists() {
            Some(file_path)
        } else {
            None
        }
    }

    /// Checks if a file exists in the cache
    ///
    /// # Arguments
    /// * `group_id` - The MLS group ID
    /// * `subdirectory` - Subdirectory within the group
    /// * `filename` - The filename to check
    #[allow(dead_code)]
    pub(crate) fn file_exists(
        &self,
        group_id: &GroupId,
        subdirectory: &str,
        filename: &str,
    ) -> bool {
        self.get_file_path(group_id, subdirectory, filename)
            .is_some()
    }

    /// Finds a file with a given prefix in the cache
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
        let group_dir = self.get_group_dir(group_id, subdirectory);

        if !group_dir.exists() {
            return None;
        }

        if let Ok(entries) = std::fs::read_dir(&group_dir) {
            for entry in entries.flatten() {
                if let Some(filename) = entry.file_name().to_str()
                    && filename.starts_with(prefix)
                {
                    return Some(entry.path());
                }
            }
        }

        None
    }

    /// Gets the cache directory for a specific group and subdirectory
    fn get_group_dir(&self, group_id: &GroupId, subdirectory: &str) -> PathBuf {
        let group_id_hex = hex::encode(group_id.as_slice());
        self.cache_dir.join(group_id_hex).join(subdirectory)
    }

    /// Returns the cache directory path
    #[allow(dead_code)]
    pub(crate) fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_storage_creation() {
        let temp_dir = TempDir::new().unwrap();
        let storage = MediaFileStorage::new(temp_dir.path()).unwrap();

        assert!(storage.cache_dir().exists());
        assert_eq!(storage.cache_dir(), temp_dir.path().join("media_cache"));
    }

    #[tokio::test]
    async fn test_store_and_retrieve_file() {
        let temp_dir = TempDir::new().unwrap();
        let storage = MediaFileStorage::new(temp_dir.path()).unwrap();

        let group_id = GroupId::from_slice(&[1u8; 8]);
        let test_data = b"test file content";

        // Store file
        let path = storage
            .store_file(&group_id, "test_subdir", "test.txt", test_data)
            .await
            .unwrap();

        // Verify it exists
        assert!(path.exists());

        // Verify content
        let content = tokio::fs::read(&path).await.unwrap();
        assert_eq!(content, test_data);

        // Verify retrieval
        let retrieved_path = storage
            .get_file_path(&group_id, "test_subdir", "test.txt")
            .unwrap();
        assert_eq!(path, retrieved_path);

        // Verify existence check
        assert!(storage.file_exists(&group_id, "test_subdir", "test.txt"));
        assert!(!storage.file_exists(&group_id, "test_subdir", "nonexistent.txt"));
    }

    #[tokio::test]
    async fn test_find_file_with_prefix() {
        let temp_dir = TempDir::new().unwrap();
        let storage = MediaFileStorage::new(temp_dir.path()).unwrap();

        let group_id = GroupId::from_slice(&[1u8; 8]);

        // Store files with different extensions
        storage
            .store_file(&group_id, "images", "abc123.jpg", b"jpeg data")
            .await
            .unwrap();
        storage
            .store_file(&group_id, "images", "abc123.png", b"png data")
            .await
            .unwrap();
        storage
            .store_file(&group_id, "images", "def456.jpg", b"other jpeg")
            .await
            .unwrap();

        // Find by prefix
        let found = storage
            .find_file_with_prefix(&group_id, "images", "abc123")
            .unwrap();
        assert!(found.to_string_lossy().contains("abc123"));

        // Non-existent prefix
        assert!(
            storage
                .find_file_with_prefix(&group_id, "images", "xyz")
                .is_none()
        );
    }

    #[tokio::test]
    async fn test_atomic_write() {
        let temp_dir = TempDir::new().unwrap();
        let storage = MediaFileStorage::new(temp_dir.path()).unwrap();

        let group_id = GroupId::from_slice(&[1u8; 8]);

        // Store file
        let path = storage
            .store_file(&group_id, "test", "atomic.txt", b"data")
            .await
            .unwrap();

        // Verify no .tmp file left behind
        let temp_path = path.with_extension("tmp");
        assert!(!temp_path.exists());

        // Verify actual file exists
        assert!(path.exists());
    }

    #[tokio::test]
    async fn test_multiple_subdirectories() {
        let temp_dir = TempDir::new().unwrap();
        let storage = MediaFileStorage::new(temp_dir.path()).unwrap();

        let group_id = GroupId::from_slice(&[1u8; 8]);

        // Store files in different subdirectories
        storage
            .store_file(&group_id, "group_images", "image.jpg", b"image data")
            .await
            .unwrap();
        storage
            .store_file(&group_id, "media", "video.mp4", b"video data")
            .await
            .unwrap();

        // Both should exist in their respective subdirectories
        assert!(storage.file_exists(&group_id, "group_images", "image.jpg"));
        assert!(storage.file_exists(&group_id, "media", "video.mp4"));

        // Should not exist in wrong subdirectory
        assert!(!storage.file_exists(&group_id, "media", "image.jpg"));
        assert!(!storage.file_exists(&group_id, "group_images", "video.mp4"));
    }
}
