use crate::database::{Database, DatabaseError};
// use crate::nostr_manager::NostrManager;
use nostr_sdk::{NostrLMDB, Options};
use anyhow::Context;
use nostr_mls::NostrMls;
use nostr_mls_sqlite_storage::NostrMlsSqliteStorage;
use nostr_sdk::Client;
use once_cell::sync::Lazy;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use thiserror::Error;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{filter::EnvFilter, fmt::Layer, prelude::*, registry::Registry};

// mod accounts;
mod database;
// mod key_packages;
// mod nostr_manager;
mod relays;
// mod media;
// mod secrets_store;
mod types;

#[derive(Error, Debug)]
pub enum WhitenoiseError {
    #[error("Directory creation error: {0}")]
    DirectoryCreation(#[from] std::io::Error),

    #[error("Logging setup error: {0}")]
    LoggingSetup(String),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Nostr client error: {0}")]
    NostrClient(#[from] nostr_sdk::client::Error),

    #[error("Database error: {0}")]
    Database(#[from] DatabaseError),

    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
}

#[derive(Clone, Debug)]
pub struct WhitenoiseConfig {
    /// Directory for application data
    pub data_dir: PathBuf,

    /// Directory for application logs
    pub logs_dir: PathBuf,
}

impl WhitenoiseConfig {
    pub fn new(data_dir: &Path, logs_dir: &Path) -> Self {
        let env_suffix = if cfg!(debug_assertions) {
            "dev"
        } else {
            "release"
        };
        let formatted_data_dir = data_dir.join(env_suffix);
        let formatted_logs_dir = logs_dir.join(env_suffix);

        Self {
            data_dir: formatted_data_dir,
            logs_dir: formatted_logs_dir,
        }
    }
}

#[derive(Clone)]
pub struct Whitenoise {
    config: WhitenoiseConfig,
    database: Arc<Database>,
    nostr: Client,
    nostr_mls: Arc<Mutex<Option<NostrMls<NostrMlsSqliteStorage>>>>,
}

impl Whitenoise {
    pub async fn new(config: WhitenoiseConfig) -> Result<Self, WhitenoiseError> {
        let data_dir = &config.data_dir;
        let logs_dir = &config.logs_dir;

        // Setup directories
        std::fs::create_dir_all(data_dir)
            .with_context(|| format!("Failed to create data directory: {:?}", data_dir))
            .map_err(WhitenoiseError::from)?;
        std::fs::create_dir_all(logs_dir)
            .with_context(|| format!("Failed to create logs directory: {:?}", logs_dir))
            .map_err(WhitenoiseError::from)?;

        let file_appender = tracing_appender::rolling::RollingFileAppender::builder()
            .rotation(tracing_appender::rolling::Rotation::DAILY)
            .filename_prefix("whitenoise")
            .filename_suffix("log")
            .build(logs_dir)
            .map_err(|e| WhitenoiseError::LoggingSetup(e.to_string()))?;

        // Setup logging
        let (non_blocking_file, file_guard) = tracing_appender::non_blocking(file_appender);
        let (non_blocking_stdout, stdout_guard) = tracing_appender::non_blocking(std::io::stdout());

        static GUARDS: Lazy<Mutex<Option<(WorkerGuard, WorkerGuard)>>> =
            Lazy::new(|| Mutex::new(None));
        *GUARDS.lock().unwrap() = Some((file_guard, stdout_guard));

        // Create a layer for stdout with ANSI color codes enabled
        let stdout_layer = Layer::new()
            .with_writer(non_blocking_stdout)
            .with_ansi(true) // Enable ANSI color codes for stdout
            .with_target(true); // Include target information in stdout logs

        // Create a layer for file output with ANSI color codes explicitly disabled
        let file_layer = Layer::new()
            .with_writer(non_blocking_file)
            .with_ansi(false) // Disable ANSI color codes for file output
            .with_target(true); // Include target information in file logs

        // Initialize the tracing subscriber registry
        Registry::default()
            .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug")))
            .with(stdout_layer)
            .with(file_layer)
            .init();

        tracing::debug!("Logging initialized in directory: {:?}", logs_dir);

        let database = Arc::new(Database::new(data_dir.join("whitenoise.sqlite")).await?);

        let client = {
            let full_path = data_dir.join("nostr_lmdb");
            let db = NostrLMDB::open(full_path).expect("Failed to open Nostr database");
            Client::builder().database(db).opts(Options::default()).build()
        };

        client.add_relay("wss://purplepag.es").await.map_err(WhitenoiseError::from)?;
        client.add_relay("wss://relay.primal.net").await.map_err(WhitenoiseError::from)?;

        let nostr_mls = Arc::new(Mutex::new(None));

        // Return fully configured, ready-to-go instance
        Ok(Self {
            config,
            database,
            nostr: client,
            nostr_mls,
        })
    }

    pub async fn delete_all_data(&self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::debug!(target: "whitenoise::delete_all_data", "Deleting all data");

        // TODO: Remove nostr cache first
        // self.nostr.delete_all_data().await?;

        // Remove database (accounts and media) data
        self.database.delete_all_data().await?;

        // Remove MLS related data
        {
            let mut nostr_mls = self.nostr_mls.lock().unwrap_or_else(|e| {
                tracing::error!("Failed to lock nostr_mls: {:?}", e);
                panic!("Mutex poisoned: {}", e);
            });

            if let Some(_mls) = nostr_mls.as_mut() {
                // Close the current MLS instance
                *nostr_mls = None;
            }
        }

        // Remove MLS related data
        let mls_dir = self.config.data_dir.join("mls");
        if mls_dir.exists() {
            tracing::debug!(
                target: "whitenoise::delete_all_data",
                "Removing MLS directory: {:?}",
                mls_dir
            );
            if let Err(e) = tokio::fs::remove_dir_all(&mls_dir).await {
                tracing::error!(
                    target: "whitenoise::delete_all_data",
                    "Failed to remove MLS directory: {:?}",
                    e
                );
                return Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to remove MLS directory: {}", e),
                )));
            }

            // Recreate the empty directory
            if let Err(e) = tokio::fs::create_dir_all(&mls_dir).await {
                tracing::error!(
                    target: "whitenoise::delete_all_data",
                    "Failed to recreate MLS directory: {:?}",
                    e
                );
                return Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to recreate MLS directory: {}", e),
                )));
            }
        }

        // Remove logs
        if self.config.logs_dir.exists() {
            for entry in std::fs::read_dir(&self.config.logs_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    std::fs::remove_file(path)?;
                } else if path.is_dir() {
                    std::fs::remove_dir_all(path)?;
                }
            }
        }

        Ok(())
    }
}

impl std::fmt::Debug for Whitenoise {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Whitenoise")
            .field("config", &self.config)
            .field("database", &self.database)
            .field("nostr", &self.nostr)
            .field("nostr_mls", &"<redacted>")
            .finish()
    }
}
