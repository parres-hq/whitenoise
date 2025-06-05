use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{migrate::MigrateDatabase, Sqlite, SqlitePool};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;

const MIGRATION_FILES: &[(&str, &[u8])] = &[
    (
        "0001_accounts.sql",
        include_bytes!("../db_migrations/0001_accounts.sql"),
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
}

#[derive(Clone, Debug)]
pub struct Database {
    pub pool: SqlitePool,
    #[allow(unused)]
    pub path: PathBuf,
    #[allow(unused)]
    pub last_connected: std::time::SystemTime,
}

impl Database {
    pub async fn new(db_path: PathBuf) -> Result<Self, DatabaseError> {
        // Create parent directories if they don't exist
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let db_url = format!("{}", db_path.display());

        // Create database if it doesn't exist
        tracing::debug!("Checking if DB exists...{:?}", db_url);
        if Sqlite::database_exists(&db_url).await.unwrap_or(false) {
            tracing::debug!("DB exists");
        } else {
            tracing::debug!("DB does not exist, creating...");
            Sqlite::create_database(&db_url).await.map_err(|e| {
                tracing::error!("Error creating DB: {:?}", e);
                DatabaseError::Sqlx(e)
            })?;
            tracing::debug!("DB created");
        }

        // Create connection pool with refined settings
        tracing::debug!("Creating connection pool...");
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
        tracing::debug!("Running migrations...");

        // Extract the parent directory from db_path as the data_dir
        let data_dir = db_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        tracing::debug!("Using data directory: {:?}", data_dir);

        // Always use embedded migrations by copying them to a temporary directory
        let temp_dir = data_dir.join("temp_migrations");
        if temp_dir.exists() {
            fs::remove_dir_all(&temp_dir)?;
        }
        fs::create_dir_all(&temp_dir)?;

        // Copy all migration files from the embedded assets
        for (filename, content) in MIGRATION_FILES {
            tracing::debug!("Writing migration file: {}", filename);
            fs::write(temp_dir.join(filename), content)?;
        }

        let migrations_path = temp_dir;
        tracing::debug!("Migrations path: {:?}", migrations_path);

        let migration_result = match sqlx::migrate::Migrator::new(migrations_path.clone()).await {
            Ok(migrator) => {
                let result = migrator.run(&pool).await;
                if result.is_ok() {
                    tracing::debug!("Migrations applied successfully");
                }
                result.map_err(DatabaseError::from)
            }
            Err(e) => {
                tracing::error!("Failed to create migrator: {:?}", e);
                Err(DatabaseError::Migrate(e))
            }
        };

        // Always clean up temp migrations directory
        if let Err(e) = fs::remove_dir_all(data_dir.join("temp_migrations")) {
            tracing::warn!("Failed to remove temp migrations directory: {:?}", e);
        }

        // Return migration result or error
        migration_result?;

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
