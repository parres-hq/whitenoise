pub use crate::accounts::{Account, AccountSettings, AccountOnboarding};
use crate::database::Database;
pub use crate::error::WhitenoiseError;
use crate::nostr_manager::NostrManager;

use anyhow::Context;
use nostr_sdk::prelude::*;
use once_cell::sync::OnceCell;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{filter::EnvFilter, fmt::Layer, prelude::*, registry::Registry};

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

mod accounts;
mod api;
mod database;
mod error;
// mod key_packages;
mod nostr_manager;
mod relays;
// mod media;
mod secrets_store;
mod types;

static TRACING_GUARDS: OnceCell<Mutex<Option<(WorkerGuard, WorkerGuard)>>> = OnceCell::new();
static TRACING_INIT: OnceCell<()> = OnceCell::new();

fn init_tracing(logs_dir: &std::path::Path) {
    TRACING_INIT.get_or_init(|| {
        let file_appender = tracing_appender::rolling::RollingFileAppender::builder()
            .rotation(tracing_appender::rolling::Rotation::DAILY)
            .filename_prefix("whitenoise")
            .filename_suffix("log")
            .build(logs_dir)
            .expect("Failed to create file appender");

        let (non_blocking_file, file_guard) = tracing_appender::non_blocking(file_appender);
        let (non_blocking_stdout, stdout_guard) = tracing_appender::non_blocking(std::io::stdout());

        TRACING_GUARDS
            .set(Mutex::new(Some((file_guard, stdout_guard))))
            .ok();

        let stdout_layer = Layer::new()
            .with_writer(non_blocking_stdout)
            .with_ansi(true)
            .with_target(true);

        let file_layer = Layer::new()
            .with_writer(non_blocking_file)
            .with_ansi(false)
            .with_target(true);

        Registry::default()
            .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
            .with(stdout_layer)
            .with(file_layer)
            .init();
    });
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
    pub config: WhitenoiseConfig,
    pub accounts: HashMap<PublicKey, Account>,
    pub active_account: Option<PublicKey>,
    pub(crate) database: Arc<Database>,
    pub(crate) nostr: NostrManager,
}

impl Whitenoise {
    /// Initializes the Whitenoise application with the provided configuration.
    ///
    /// This method sets up the necessary data and log directories, configures logging,
    /// initializes the database, and sets up the Nostr client with appropriate relays
    /// based on the build environment (development or release).
    ///
    /// # Arguments
    ///
    /// * `config` - A [`WhitenoiseConfig`] struct specifying the data and log directories.
    ///
    /// # Returns
    ///
    /// Returns a [`Result`] containing a fully initialized [`Whitenoise`] instance on success,
    /// or a [`WhitenoiseError`] if initialization fails at any step.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The data or log directories cannot be created.
    /// - Logging cannot be set up.
    /// - The database cannot be initialized.
    /// - The Nostr client cannot be configured or fails to connect to relays.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use whitenoise::{Whitenoise, WhitenoiseConfig};
    /// # use std::path::Path;
    /// # async fn example() -> Result<(), whitenoise::WhitenoiseError> {
    /// let config = WhitenoiseConfig::new(Path::new("./data"), Path::new("./logs"));
    /// let whitenoise = Whitenoise::initialize_whitenoise(config).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn initialize_whitenoise(config: WhitenoiseConfig) -> Result<Self, WhitenoiseError> {
        let data_dir = &config.data_dir;
        let logs_dir = &config.logs_dir;

        // Setup directories
        std::fs::create_dir_all(data_dir)
            .with_context(|| format!("Failed to create data directory: {:?}", data_dir))
            .map_err(WhitenoiseError::from)?;
        std::fs::create_dir_all(logs_dir)
            .with_context(|| format!("Failed to create logs directory: {:?}", logs_dir))
            .map_err(WhitenoiseError::from)?;

        // Only initialize tracing once
        init_tracing(logs_dir);

        tracing::debug!("Logging initialized in directory: {:?}", logs_dir);

        let database = Arc::new(Database::new(data_dir.join("whitenoise.sqlite")).await?);
        let nostr = NostrManager::new(data_dir.join("nostr_lmdb")).await?;

        // TODO: Load accounts from database

        // Return fully configured, ready-to-go instance
        Ok(Self {
            config,
            database,
            nostr,
            accounts: HashMap::new(),
            active_account: None,
        })
    }

    /// Deletes all application data, including the database, MLS data, and log files.
    ///
    /// This asynchronous method removes all persistent data associated with the Whitenoise instance.
    /// It deletes the nostr cache, database, MLS-related directories, and all log files. If the MLS directory exists,
    /// it is removed and then recreated as an empty directory. This is useful for resetting the application
    /// to a clean state.
    ///
    /// # Returns
    ///
    /// Returns a [`Result`] which is `Ok(())` if all data is successfully deleted, or an error boxed as
    /// [`Box<dyn std::error::Error>`] if any step fails.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The Nostr cache cannot be deleted.
    /// - The database data cannot be deleted.
    /// - The MLS directory cannot be removed or recreated.
    /// - Log files or directories cannot be deleted.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use whitenoise::{Whitenoise, WhitenoiseConfig};
    /// # use std::path::Path;
    /// # async fn example(whitenoise: Whitenoise) -> Result<(), Box<dyn std::error::Error>> {
    /// whitenoise.delete_all_data().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn delete_all_data(&self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::debug!(target: "whitenoise::delete_all_data", "Deleting all data");

        // Remove nostr cache first
        self.nostr.delete_all_data().await?;

        // Remove database (accounts and media) data
        self.database.delete_all_data().await?;

        // Remove MLS related data
        // TODO: MOVE TO ACCOUNTS
        // {
        //     let mut nostr_mls = self.nostr_mls.lock().unwrap_or_else(|e| {
        //         tracing::error!("Failed to lock nostr_mls: {:?}", e);
        //         panic!("Mutex poisoned: {}", e);
        //     });

        //     if let Some(_mls) = nostr_mls.as_mut() {
        //         // Close the current MLS instance
        //         *nostr_mls = None;
        //     }
        // }

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
                return Err(Box::new(std::io::Error::other(format!(
                    "Failed to remove MLS directory: {}",
                    e
                ))));
            }

            // Recreate the empty directory
            if let Err(e) = tokio::fs::create_dir_all(&mls_dir).await {
                tracing::error!(
                    target: "whitenoise::delete_all_data",
                    "Failed to recreate MLS directory: {:?}",
                    e
                );
                return Err(Box::new(std::io::Error::other(format!(
                    "Failed to recreate MLS directory: {}",
                    e
                ))));
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
            .field("accounts", &self.accounts)
            .field("active_account", &self.active_account)
            .field("database", &"<REDACTED>")
            .field("nostr", &"<REDACTED>")
            .finish()
    }
}
