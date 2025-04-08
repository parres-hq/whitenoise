use crate::database::Database;
use crate::nostr_manager::NostrManager;
use nostr_openmls::NostrMls;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::AppHandle;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct Whitenoise {
    pub database: Arc<Database>,
    pub nostr: NostrManager,
    pub nostr_mls: Arc<Mutex<NostrMls>>,
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
            nostr_mls: Arc::new(Mutex::new(NostrMls::new(data_dir.clone(), None))),
            data_dir,
            logs_dir,
        }
    }

    pub async fn delete_all_data(&self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::debug!(target: "whitenoise::delete_all_data", "Deleting all data");

        // Clear data first
        self.nostr.delete_all_data(&self.data_dir).await?;
        self.database.delete_all_data().await?;
        self.nostr_mls.lock().await.delete_all_data()?;

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
