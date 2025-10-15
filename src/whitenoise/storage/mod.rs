pub mod media_files;

use crate::whitenoise::error::Result;
use std::path::Path;

/// Storage layer for managing filesystem operations
///
/// This struct provides access to various storage subsystems
pub struct Storage {
    pub(crate) media_files: media_files::MediaFileStorage,
}

impl Storage {
    /// Creates a new Storage instance
    ///
    /// # Arguments
    /// * `data_dir` - The application data directory
    ///
    /// # Returns
    /// A new Storage instance with all subsystems initialized
    pub(crate) async fn new(data_dir: &Path) -> Result<Self> {
        Ok(Self {
            media_files: media_files::MediaFileStorage::new(data_dir).await?,
        })
    }

    /// Removes all storage artifacts (media cache, etc.)
    ///
    /// This is used when deleting all application data.
    /// Cached directories will be automatically recreated when needed.
    ///
    /// # Returns
    /// Ok(()) on success
    ///
    /// # Errors
    /// Returns error if filesystem operations fail
    pub(crate) async fn wipe_all(&self) -> Result<()> {
        self.media_files.wipe_all().await?;
        Ok(())
    }
}
