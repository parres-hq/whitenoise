use thiserror::Error;

#[derive(Error, Debug)]
pub enum MediaError {
    #[error("Cache error: {0}")]
    Cache(String),

    #[error("Upload error: {0}")]
    Upload(String),

    #[error("Delete error: {0}")]
    Delete(String),

    #[error("Export secret error: {0}")]
    ExportSecret(String),

    #[error("Metadata error: {0}")]
    Metadata(String),

    #[error("Sanitization error: {0}")]
    Sanitize(String),

    // #[error("Failed to download file: {0}")]
    // Download(String),
    #[error("Failed to generate IMETA tag: {0}")]
    Encryption(String),

    #[error("Failed to decrypt file: {0}")]
    Decryption(String),

    #[error("Database operation failed: {0}")]
    Database(#[from] sqlx::Error),
}
