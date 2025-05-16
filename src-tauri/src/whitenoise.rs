use crate::database::Database;
use crate::nostr_manager::NostrManager;
use nostr_mls::NostrMls;
use nostr_mls_sqlite_storage::NostrMlsSqliteStorage;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::AppHandle;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct Whitenoise {
    pub database: Arc<Database>,
    pub nostr: NostrManager,
    pub nostr_mls: Arc<Mutex<Option<NostrMls<NostrMlsSqliteStorage>>>>,
    pub data_dir: PathBuf,
    pub logs_dir: PathBuf,
}

impl Whitenoise {
    pub async fn new(data_dir: PathBuf, logs_dir: PathBuf, app_handle: AppHandle) -> Self {
        tracing::info!(
            target: "whitenoise::whitenoise::new",
            "Creating Whitenoise instance with data_dir: {:?}",
            &data_dir
        );

        Self {
            database: Arc::new(
                Database::new(data_dir.join("whitenoise.sqlite"), app_handle.clone())
                    .await
                    .expect("Failed to create database"),
            ),
            nostr: NostrManager::new(data_dir.clone(), app_handle.clone())
                .await
                .expect("Failed to create Nostr manager"),
            nostr_mls: Arc::new(Mutex::new(None)),
            data_dir,
            logs_dir,
        }
    }

    pub async fn delete_all_data(&self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::debug!(target: "whitenoise::delete_all_data", "Deleting all data");

        // Remove nostr cache first
        #[cfg(any(target_os = "ios", target_os = "macos"))]
        {
            self.nostr.delete_all_data(&self.data_dir).await?;
        }
        #[cfg(not(any(target_os = "ios", target_os = "macos")))]
        {
            self.nostr.delete_all_data().await?;
        }

        // Remove database (accounts and media) data
        self.database.delete_all_data().await?;

        // Remove MLS related data
        {
            let mut nostr_mls = self.nostr_mls.lock().await;
            if let Some(_mls) = nostr_mls.as_mut() {
                // Close the current MLS instance
                *nostr_mls = None;
            }

            // Delete the MLS directory which contains SQLite storage files
            let mls_dir = self.data_dir.join("mls");
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
        }

        // Remove logs
        if self.logs_dir.exists() {
            for entry in std::fs::read_dir(&self.logs_dir)? {
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
