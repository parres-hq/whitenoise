use crate::accounts::AccountError;
use crate::database::DatabaseError;
use crate::secrets_store::SecretsStoreError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum WhitenoiseError {
    #[error("Directory creation error: {0}")]
    DirectoryCreation(#[from] std::io::Error),

    #[error("Logging setup error: {0}")]
    LoggingSetup(String),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Nostr MLS SQLite storage error: {0}")]
    NostrMlsSqliteStorage(#[from] nostr_mls_sqlite_storage::error::Error),

    #[error("Secrets store error: {0}")]
    SecretsStore(#[from] SecretsStoreError),

    #[error("Nostr client error: {0}")]
    NostrClient(#[from] nostr_sdk::client::Error),

    #[error("Nostr key error: {0}")]
    NostrKey(#[from] nostr_sdk::key::Error),

    #[error("Database error: {0}")]
    Database(#[from] DatabaseError),

    #[error("Account error: {0}")]
    Account(#[from] AccountError),

    #[error("SQLx error: {0}")]
    SqlxError(#[from] sqlx::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
}
