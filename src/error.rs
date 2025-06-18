use crate::accounts::AccountError;
use crate::database::DatabaseError;
use crate::nostr_manager::NostrManagerError;
use crate::secrets_store::SecretsStoreError;
use thiserror::Error;

pub type Result<T> = core::result::Result<T, WhitenoiseError>;

#[derive(Error, Debug)]
pub enum WhitenoiseError {
    #[error("Filesystem error: {0}")]
    Filesystem(#[from] std::io::Error),

    #[error("Logging setup error: {0}")]
    LoggingSetup(String),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Contact list error: {0}")]
    ContactList(String),

    #[error("Nostr MLS SQLite storage error: {0}")]
    NostrMlsSqliteStorage(#[from] nostr_mls_sqlite_storage::error::Error),

    #[error("Account not found")]
    AccountNotFound,

    #[error("Account not authorized")]
    AccountNotAuthorized,

    #[error("NostrMls not initialized")]
    NostrMlsNotInitialized,

    #[error("Nostr Mls error: {0}")]
    NostrMlsError(#[from] nostr_mls::Error),

    #[error("Nostr Event error: {0}")]
    NostrEventBuilderError(#[from] nostr::event::builder::Error),

    #[error("Secrets store error: {0}")]
    SecretsStore(#[from] SecretsStoreError),

    #[error("Nostr client error: {0}")]
    NostrClient(#[from] nostr_sdk::client::Error),

    #[error("Nostr key error: {0}")]
    NostrKey(#[from] nostr_sdk::key::Error),

    #[error("Nostr url error: {0}")]
    NostrUrl(#[from] nostr::types::url::Error),

    #[error("Database error: {0}")]
    Database(#[from] DatabaseError),

    #[error("Account error: {0}")]
    Account(#[from] AccountError),

    #[error("SQLx error: {0}")]
    SqlxError(#[from] sqlx::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Nostr manager error: {0}")]
    NostrManager(#[from] NostrManagerError),

    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
}

impl From<Box<dyn std::error::Error + Send + Sync>> for WhitenoiseError {
    fn from(err: Box<dyn std::error::Error + Send + Sync>) -> Self {
        WhitenoiseError::Other(anyhow::anyhow!(err.to_string()))
    }
}
