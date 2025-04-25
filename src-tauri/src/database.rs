use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{migrate::MigrateDatabase, Sqlite, SqlitePool};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tauri::{path::BaseDirectory, AppHandle, Manager};
use thiserror::Error;

const MIGRATION_FILES: &[(&str, &[u8])] = &[
    (
        "0001_initial.sql",
        include_bytes!("../db_migrations/0001_initial.sql"),
    ),
    (
        "0002_add_media_files.sql",
        include_bytes!("../db_migrations/0002_add_media_files.sql"),
    ),
    // Add new migrations here in order, for example:
    // ("000X_something.sql", include_bytes!("../db_migrations/000X_something.sql")),
    // ("000Y_another.sql", include_bytes!("../db_migrations/000Y_another.sql")),
];

#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("File system error: {0}")]
    FileSystem(#[from] std::io::Error),
    #[error("SQLx error: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("Migrate error: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("Tauri error: {0}")]
    Tauri(#[from] tauri::Error),
}

#[derive(Clone)]
pub struct Database {
    pub pool: SqlitePool,
    #[allow(unused)]
    pub path: PathBuf,
    #[allow(unused)]
    pub last_connected: std::time::SystemTime,
}

impl Database {
    pub async fn new(db_path: PathBuf, app_handle: AppHandle) -> Result<Self, DatabaseError> {
        // Create parent directories if they don't exist
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let db_url = format!("{}", db_path.display());

        // Create database if it doesn't exist
        tracing::info!("Checking if DB exists...{:?}", db_url);
        if Sqlite::database_exists(&db_url).await.unwrap_or(false) {
            tracing::info!("DB exists");
        } else {
            tracing::info!("DB does not exist, creating...");
            match Sqlite::create_database(&db_url).await {
                Ok(_) => {
                    tracing::info!("DB created");
                }
                Err(e) => {
                    tracing::error!("Error creating DB: {:?}", e);
                }
            }
        }

        // Create connection pool with refined settings
        tracing::info!("Creating connection pool...");
        let pool = SqlitePoolOptions::new()
            .acquire_timeout(Duration::from_secs(5))
            .max_connections(10)
            .after_connect(|conn, _| {
                Box::pin(async move {
                    let conn = &mut *conn;
                    // Enable WAL mode
                    sqlx::query("PRAGMA journal_mode=WAL")
                        .execute(&mut *conn)
                        .await?;
                    // Set busy timeout
                    sqlx::query("PRAGMA busy_timeout=5000")
                        .execute(&mut *conn)
                        .await?;
                    // Enable foreign keys and triggers
                    sqlx::query("PRAGMA foreign_keys = ON;")
                        .execute(&mut *conn)
                        .await?;
                    sqlx::query("PRAGMA recursive_triggers = ON;")
                        .execute(&mut *conn)
                        .await?;
                    Ok(())
                })
            })
            .connect(&format!("{}?mode=rwc", db_url))
            .await?;

        // Run migrations
        tracing::info!("Running migrations...");

        let migrations_path = if cfg!(target_os = "android") {
            // On Android, we need to copy migrations to a temporary directory
            let temp_dir = app_handle.path().app_data_dir()?.join("temp_migrations");
            if temp_dir.exists() {
                fs::remove_dir_all(&temp_dir)?;
            }
            fs::create_dir_all(&temp_dir)?;

            // Copy all migration files from the embedded assets
            for (filename, content) in MIGRATION_FILES {
                tracing::info!("Writing migration file: {}", filename);
                fs::write(temp_dir.join(filename), content)?;
            }

            temp_dir
        } else {
            app_handle
                .path()
                .resolve("db_migrations", BaseDirectory::Resource)?
        };

        tracing::info!("Migrations path: {:?}", migrations_path);
        if !migrations_path.exists() {
            tracing::error!("Migrations directory not found at {:?}", migrations_path);
            return Err(DatabaseError::FileSystem(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Migrations directory not found at {:?}", migrations_path),
            )));
        }

        match sqlx::migrate::Migrator::new(migrations_path).await {
            Ok(migrator) => {
                migrator.run(&pool).await?;
                tracing::info!("Migrations applied successfully");
                // Clean up temp directory on Android after successful migration
                if cfg!(target_os = "android") {
                    if let Ok(temp_dir) = app_handle.path().app_data_dir() {
                        let _ = fs::remove_dir_all(temp_dir.join("temp_migrations"));
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to create migrator: {:?}", e);
                return Err(DatabaseError::Migrate(e));
            }
        }

        Ok(Self {
            pool,
            path: db_path,
            last_connected: std::time::SystemTime::now(),
        })
    }

    pub async fn delete_all_data(&self) -> Result<(), DatabaseError> {
        let mut txn = self.pool.begin().await?;

        // Disable foreign key constraints temporarily
        sqlx::query("PRAGMA foreign_keys = OFF")
            .execute(&mut *txn)
            .await?;

        // Delete data in reverse order of dependencies
        sqlx::query("DELETE FROM media_files")
            .execute(&mut *txn)
            .await?;
        sqlx::query("DELETE FROM account_relays")
            .execute(&mut *txn)
            .await?;
        sqlx::query("DELETE FROM accounts")
            .execute(&mut *txn)
            .await?;

        // Re-enable foreign key constraints
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&mut *txn)
            .await?;

        txn.commit().await?;
        Ok(())
    }
}
