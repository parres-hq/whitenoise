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
    pub(crate) fn new(data_dir: &Path) -> Result<Self> {
        Ok(Self {
            media_files: media_files::MediaFileStorage::new(data_dir)?,
        })
    }
}
