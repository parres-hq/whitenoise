use thiserror::Error;

#[derive(Error, Debug)]
pub enum MediaError {
    #[error("Failed to upload file: {0}")]
    Upload(String),
    #[error("Failed to download file: {0}")]
    Download(String),
    #[error("Failed to get export secret: {0}")]
    ExportSecret(String),
    #[error("Failed to generate IMETA tag: {0}")]
    Metadata(String),
    #[error("Failed to encrypt file: {0}")]
    Encryption(String),
    #[error("Failed to decrypt file: {0}")]
    Decryption(String),
    #[error("Cache operation failed: {0}")]
    Cache(String),
    #[error("Database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Failed to delete file: {0}")]
    Delete(String),
}
