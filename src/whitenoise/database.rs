use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{migrate::MigrateDatabase, Sqlite, SqlitePool};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;

const MIGRATION_FILES: &[(&str, &[u8])] = &[
    (
        "0001_accounts.sql",
        include_bytes!("../../db_migrations/0001_accounts.sql"),
    ),
    (
        "0002_add_media_files.sql",
        include_bytes!("../../db_migrations/0002_add_media_files.sql"),
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
        // media_files has foreign key to accounts, so delete it first
        sqlx::query("DELETE FROM media_files")
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    async fn create_test_db() -> (Database, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let db_path = temp_dir.path().join("test.db");
        let db = Database::new(db_path)
            .await
            .expect("Failed to create test database");
        (db, temp_dir)
    }

    #[tokio::test]
    async fn test_database_creation() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let db_path = temp_dir.path().join("test.db");

        // Database should be created successfully
        let db = Database::new(db_path.clone()).await;
        assert!(db.is_ok());

        let db = db.unwrap();
        assert_eq!(db.path, db_path);
        assert!(db.last_connected.elapsed().unwrap().as_secs() < 2);
    }

    #[tokio::test]
    async fn test_database_creation_with_nested_path() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let db_path = temp_dir.path().join("nested").join("path").join("test.db");

        // Database should be created successfully even with nested directories
        let db = Database::new(db_path.clone()).await;
        assert!(db.is_ok());

        let db = db.unwrap();
        assert_eq!(db.path, db_path);
        assert!(db_path.exists());
    }

    #[tokio::test]
    async fn test_database_migrations_applied() {
        let (db, _temp_dir) = create_test_db().await;

        // Check that the accounts table exists (from migration 0001)
        let result =
            sqlx::query("SELECT name FROM sqlite_master WHERE type='table' AND name='accounts'")
                .fetch_optional(&db.pool)
                .await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_some());

        // Check that the media_files table exists (from migration 0002)
        let result =
            sqlx::query("SELECT name FROM sqlite_master WHERE type='table' AND name='media_files'")
                .fetch_optional(&db.pool)
                .await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_database_pragma_settings() {
        let (db, _temp_dir) = create_test_db().await;

        // Check that foreign keys are enabled
        let foreign_keys: (i64,) = sqlx::query_as("PRAGMA foreign_keys")
            .fetch_one(&db.pool)
            .await
            .expect("Failed to check foreign_keys pragma");
        assert_eq!(foreign_keys.0, 1);

        // Check that recursive triggers are enabled
        let recursive_triggers: (i64,) = sqlx::query_as("PRAGMA recursive_triggers")
            .fetch_one(&db.pool)
            .await
            .expect("Failed to check recursive_triggers pragma");
        assert_eq!(recursive_triggers.0, 1);

        // Check that WAL mode is enabled
        let journal_mode: (String,) = sqlx::query_as("PRAGMA journal_mode")
            .fetch_one(&db.pool)
            .await
            .expect("Failed to check journal_mode pragma");
        assert_eq!(journal_mode.0.to_lowercase(), "wal");
    }

    #[tokio::test]
    async fn test_delete_all_data() {
        let (db, _temp_dir) = create_test_db().await;

        // Insert some test data
        sqlx::query("INSERT INTO accounts (pubkey, settings, onboarding, last_synced) VALUES ('test-pubkey', '{}', '{}', 0)")
            .execute(&db.pool)
            .await
            .expect("Failed to insert test account");

        sqlx::query("INSERT INTO media_files (mls_group_id, account_pubkey, file_path, file_hash, created_at, file_metadata) VALUES (x'deadbeef', 'test-pubkey', '/path/test.jpg', 'test-hash', 1234567890, '{}')")
            .execute(&db.pool)
            .await
            .expect("Failed to insert test media file");

        // Verify data exists
        let account_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM accounts")
            .fetch_one(&db.pool)
            .await
            .expect("Failed to count accounts");
        assert_eq!(account_count.0, 1);

        let media_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM media_files")
            .fetch_one(&db.pool)
            .await
            .expect("Failed to count media files");
        assert_eq!(media_count.0, 1);

        // Delete all data
        let result = db.delete_all_data().await;
        assert!(result.is_ok());

        // Verify data is deleted
        let account_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM accounts")
            .fetch_one(&db.pool)
            .await
            .expect("Failed to count accounts after deletion");
        assert_eq!(account_count.0, 0);

        let media_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM media_files")
            .fetch_one(&db.pool)
            .await
            .expect("Failed to count media files after deletion");
        assert_eq!(media_count.0, 0);
    }

    #[tokio::test]
    async fn test_database_connection_pool() {
        let (db, _temp_dir) = create_test_db().await;

        // Test that we can acquire multiple connections
        let mut connections = Vec::new();

        for _ in 0..5 {
            let conn = db.pool.acquire().await;
            assert!(conn.is_ok());
            connections.push(conn.unwrap());
        }

        // Test that we can execute queries on different connections
        for mut conn in connections {
            let result: (i64,) = sqlx::query_as("SELECT 1")
                .fetch_one(&mut *conn)
                .await
                .expect("Failed to execute query on connection");
            assert_eq!(result.0, 1);
        }
    }

    #[tokio::test]
    async fn test_database_reopen_existing() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let db_path = temp_dir.path().join("test.db");

        // Create database first time
        let db1 = Database::new(db_path.clone())
            .await
            .expect("Failed to create database");

        // Insert some data
        sqlx::query("INSERT INTO accounts (pubkey, settings, onboarding, last_synced) VALUES ('test-pubkey', '{}', '{}', 0)")
            .execute(&db1.pool)
            .await
            .expect("Failed to insert test data");

        drop(db1);

        // Reopen the same database
        let db2 = Database::new(db_path.clone())
            .await
            .expect("Failed to reopen database");

        // Verify data persists
        let account_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM accounts")
            .fetch_one(&db2.pool)
            .await
            .expect("Failed to count accounts");
        assert_eq!(account_count.0, 1);

        // Verify the account data
        let account: (String, String, String, i64) =
            sqlx::query_as("SELECT pubkey, settings, onboarding, last_synced FROM accounts")
                .fetch_one(&db2.pool)
                .await
                .expect("Failed to fetch account");
        assert_eq!(account.0, "test-pubkey");
        assert_eq!(account.1, "{}");
        assert_eq!(account.2, "{}");
        assert_eq!(account.3, 0);
    }

    #[tokio::test]
    async fn test_database_error_handling() {
        // Test with invalid path (this should still work as SQLite is quite permissive)
        let invalid_path = PathBuf::from("/invalid/path/that/should/fail.db");
        let result = Database::new(invalid_path).await;

        // This might succeed or fail depending on permissions, but shouldn't panic
        match result {
            Ok(_) => {
                // If it succeeds, that's fine too (SQLite might create the path)
            }
            Err(e) => {
                // Should be a proper DatabaseError
                match e {
                    DatabaseError::FileSystem(_) | DatabaseError::Sqlx(_) => {
                        // Expected error types
                    }
                    _ => panic!("Unexpected error type: {:?}", e),
                }
            }
        }
    }

    #[tokio::test]
    async fn test_database_clone() {
        let (db, _temp_dir) = create_test_db().await;

        // Test that Database can be cloned
        let db_clone = db.clone();

        // Both should have the same path
        assert_eq!(db.path, db_clone.path);

        // Both should be able to execute queries
        let result1: (i64,) = sqlx::query_as("SELECT 1")
            .fetch_one(&db.pool)
            .await
            .expect("Failed to execute query on original");

        let result2: (i64,) = sqlx::query_as("SELECT 2")
            .fetch_one(&db_clone.pool)
            .await
            .expect("Failed to execute query on clone");

        assert_eq!(result1.0, 1);
        assert_eq!(result2.0, 2);
    }
}
