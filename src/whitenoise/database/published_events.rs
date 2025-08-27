use chrono::{DateTime, Utc};
use nostr_sdk::EventId;

use super::{Database, DatabaseError};

/// Row structure for published_events table
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct PublishedEvent {
    pub id: i64,
    pub event_id: EventId,
    pub account_id: i64,
    pub created_at: DateTime<Utc>,
}

impl<'r, R> sqlx::FromRow<'r, R> for PublishedEvent
where
    R: sqlx::Row,
    &'r str: sqlx::ColumnIndex<R>,
    i64: sqlx::Decode<'r, <R as sqlx::Row>::Database> + sqlx::Type<<R as sqlx::Row>::Database>,
    String: sqlx::Decode<'r, <R as sqlx::Row>::Database> + sqlx::Type<<R as sqlx::Row>::Database>,
{
    fn from_row(row: &'r R) -> Result<Self, sqlx::Error> {
        let id: i64 = row.try_get("id")?;
        let event_id_hex: String = row.try_get("event_id")?;
        let account_id: i64 = row.try_get("account_id")?;
        let created_at_timestamp: i64 = row.try_get("created_at")?;

        let event_id =
            EventId::from_hex(&event_id_hex).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        let created_at = DateTime::from_timestamp(created_at_timestamp, 0)
            .ok_or_else(|| sqlx::Error::Decode("Invalid timestamp".into()))?;

        Ok(PublishedEvent {
            id,
            event_id,
            account_id,
            created_at,
        })
    }
}

/// Row structure for processed_events table
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ProcessedEvent {
    pub id: i64,
    pub event_id: EventId,
    pub account_id: i64,
    pub processed_at: DateTime<Utc>,
}

impl<'r, R> sqlx::FromRow<'r, R> for ProcessedEvent
where
    R: sqlx::Row,
    &'r str: sqlx::ColumnIndex<R>,
    i64: sqlx::Decode<'r, <R as sqlx::Row>::Database> + sqlx::Type<<R as sqlx::Row>::Database>,
    String: sqlx::Decode<'r, <R as sqlx::Row>::Database> + sqlx::Type<<R as sqlx::Row>::Database>,
{
    fn from_row(row: &'r R) -> Result<Self, sqlx::Error> {
        let id: i64 = row.try_get("id")?;
        let event_id_hex: String = row.try_get("event_id")?;
        let account_id: i64 = row.try_get("account_id")?;
        let processed_at_timestamp: i64 = row.try_get("processed_at")?;

        let event_id =
            EventId::from_hex(&event_id_hex).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        let processed_at = DateTime::from_timestamp(processed_at_timestamp, 0)
            .ok_or_else(|| sqlx::Error::Decode("Invalid timestamp".into()))?;

        Ok(ProcessedEvent {
            id,
            event_id,
            account_id,
            processed_at,
        })
    }
}

impl PublishedEvent {
    /// Records that we published a specific event to prevent processing our own events
    pub(crate) async fn create(
        event_id: &EventId,
        account_id: i64,
        database: &Database,
    ) -> Result<(), DatabaseError> {
        sqlx::query("INSERT OR IGNORE INTO published_events (event_id, account_id) VALUES (?, ?)")
            .bind(event_id.to_hex())
            .bind(account_id)
            .execute(&database.pool)
            .await?;

        tracing::debug!(
            target: "whitenoise::database::published_events::create",
            "Recorded published event: {} by account ID {}",
            event_id.to_hex(),
            account_id
        );

        Ok(())
    }

    /// Checks if we published a specific event
    pub(crate) async fn exists(
        event_id: &EventId,
        account_id: i64,
        database: &Database,
    ) -> Result<bool, DatabaseError> {
        let result: Option<(bool,)> = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM published_events WHERE event_id = ? AND account_id = ?)",
        )
        .bind(event_id.to_hex())
        .bind(account_id)
        .fetch_optional(&database.pool)
        .await?;

        Ok(result.map(|(exists,)| exists).unwrap_or(false))
    }
}

impl ProcessedEvent {
    /// Records that we processed a specific event to ensure idempotency
    pub(crate) async fn create(
        event_id: &EventId,
        account_id: i64,
        database: &Database,
    ) -> Result<(), DatabaseError> {
        // Use INSERT OR IGNORE to handle potential race conditions
        sqlx::query("INSERT OR IGNORE INTO processed_events (event_id, account_id) VALUES (?, ?)")
            .bind(event_id.to_hex())
            .bind(account_id)
            .execute(&database.pool)
            .await?;

        Ok(())
    }

    /// Checks if we already processed a specific event
    pub(crate) async fn exists(
        event_id: &EventId,
        account_id: i64,
        database: &Database,
    ) -> Result<bool, DatabaseError> {
        let result: Option<(bool,)> = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM processed_events WHERE event_id = ? AND account_id = ?)",
        )
        .bind(event_id.to_hex())
        .bind(account_id)
        .fetch_optional(&database.pool)
        .await?;

        Ok(result.map(|(exists,)| exists).unwrap_or(false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use nostr_sdk::{EventId, Keys};
    use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
    use std::str::FromStr;

    // Helper function to create a test database with the required tables
    async fn setup_test_db() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();

        // Create accounts table (referenced by foreign keys)
        sqlx::query(
            "CREATE TABLE accounts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                pubkey TEXT NOT NULL,
                user_id INTEGER NOT NULL,
                last_synced_at INTEGER,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        // Create published_events table
        sqlx::query(
            "CREATE TABLE published_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                event_id TEXT NOT NULL
                    CHECK (length(event_id) = 64 AND event_id GLOB '[0-9a-fA-F]*'),
                account_id INTEGER NOT NULL,
                created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE CASCADE,
                UNIQUE(event_id, account_id)
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        // Create processed_events table
        sqlx::query(
            "CREATE TABLE processed_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                event_id TEXT NOT NULL
                    CHECK (length(event_id) = 64 AND event_id GLOB '[0-9a-fA-F]*'),
                account_id INTEGER NOT NULL,
                processed_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE CASCADE,
                UNIQUE(event_id, account_id)
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        // Create test account
        sqlx::query(
            "INSERT INTO accounts (pubkey, user_id, created_at, updated_at)
             VALUES (?, 1, ?, ?)",
        )
        .bind("test_pubkey")
        .bind(Utc::now().timestamp())
        .bind(Utc::now().timestamp())
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    // Helper function to create a test event ID
    fn create_test_event_id() -> EventId {
        let keys = Keys::generate();
        EventId::from_str(&keys.public_key().to_string()).unwrap_or_else(|_| {
            // Fallback to a valid hex string
            EventId::from_hex("1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef")
                .unwrap()
        })
    }

    // Helper function to wrap pool in Database struct
    fn wrap_pool_in_database(pool: SqlitePool) -> Database {
        Database {
            pool,
            path: std::path::PathBuf::from(":memory:"),
            last_connected: std::time::SystemTime::now(),
        }
    }

    #[tokio::test]
    async fn test_published_event_from_row_valid_data() {
        let pool = setup_test_db().await;
        let event_id = create_test_event_id();
        let account_id = 1i64;
        let timestamp = Utc::now().timestamp();

        // Insert a test record
        sqlx::query(
            "INSERT INTO published_events (event_id, account_id, created_at) VALUES (?, ?, ?)",
        )
        .bind(event_id.to_hex())
        .bind(account_id)
        .bind(timestamp)
        .execute(&pool)
        .await
        .unwrap();

        // Fetch and verify
        let row: PublishedEvent = sqlx::query_as(
            "SELECT id, event_id, account_id, created_at FROM published_events WHERE account_id = ?",
        )
        .bind(account_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(row.event_id, event_id);
        assert_eq!(row.account_id, account_id);
        assert_eq!(row.created_at.timestamp(), timestamp);
    }

    #[tokio::test]
    async fn test_published_event_from_row_invalid_event_id() {
        let pool = setup_test_db().await;
        let account_id = 1i64;
        let timestamp = Utc::now().timestamp();

        // Insert a record with invalid event_id (should fail due to database constraints)
        let result = sqlx::query(
            "INSERT INTO published_events (event_id, account_id, created_at) VALUES (?, ?, ?)",
        )
        .bind("invalid_hex")
        .bind(account_id)
        .bind(timestamp)
        .execute(&pool)
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_processed_event_from_row_valid_data() {
        let pool = setup_test_db().await;
        let event_id = create_test_event_id();
        let account_id = 1i64;
        let timestamp = Utc::now().timestamp();

        // Insert a test record
        sqlx::query(
            "INSERT INTO processed_events (event_id, account_id, processed_at) VALUES (?, ?, ?)",
        )
        .bind(event_id.to_hex())
        .bind(account_id)
        .bind(timestamp)
        .execute(&pool)
        .await
        .unwrap();

        // Fetch and verify
        let row: ProcessedEvent = sqlx::query_as(
            "SELECT id, event_id, account_id, processed_at FROM processed_events WHERE account_id = ?",
        )
        .bind(account_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(row.event_id, event_id);
        assert_eq!(row.account_id, account_id);
        assert_eq!(row.processed_at.timestamp(), timestamp);
    }

    #[tokio::test]
    async fn test_published_event_create() {
        let pool = setup_test_db().await;
        let database = wrap_pool_in_database(pool);
        let event_id = create_test_event_id();
        let account_id = 1i64;

        // Create a published event
        let result = PublishedEvent::create(&event_id, account_id, &database).await;
        assert!(result.is_ok());

        // Verify it was inserted
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM published_events WHERE event_id = ? AND account_id = ?",
        )
        .bind(event_id.to_hex())
        .bind(account_id)
        .fetch_one(&database.pool)
        .await
        .unwrap();

        assert_eq!(count.0, 1);
    }

    #[tokio::test]
    async fn test_published_event_create_duplicate_ignored() {
        let pool = setup_test_db().await;
        let database = wrap_pool_in_database(pool);
        let event_id = create_test_event_id();
        let account_id = 1i64;

        // Create the same published event twice
        let result1 = PublishedEvent::create(&event_id, account_id, &database).await;
        let result2 = PublishedEvent::create(&event_id, account_id, &database).await;

        assert!(result1.is_ok());
        assert!(result2.is_ok());

        // Verify only one record exists (INSERT OR IGNORE behavior)
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM published_events WHERE event_id = ? AND account_id = ?",
        )
        .bind(event_id.to_hex())
        .bind(account_id)
        .fetch_one(&database.pool)
        .await
        .unwrap();

        assert_eq!(count.0, 1);
    }

    #[tokio::test]
    async fn test_published_event_exists_true() {
        let pool = setup_test_db().await;
        let database = wrap_pool_in_database(pool);
        let event_id = create_test_event_id();
        let account_id = 1i64;

        // Create a published event
        PublishedEvent::create(&event_id, account_id, &database)
            .await
            .unwrap();

        // Check if it exists
        let exists = PublishedEvent::exists(&event_id, account_id, &database)
            .await
            .unwrap();

        assert!(exists);
    }

    #[tokio::test]
    async fn test_published_event_exists_false() {
        let pool = setup_test_db().await;
        let database = wrap_pool_in_database(pool);
        let event_id = create_test_event_id();
        let account_id = 1i64;

        // Check if non-existent event exists
        let exists = PublishedEvent::exists(&event_id, account_id, &database)
            .await
            .unwrap();

        assert!(!exists);
    }

    #[tokio::test]
    async fn test_published_event_exists_different_account() {
        let pool = setup_test_db().await;
        let database = wrap_pool_in_database(pool);
        let event_id = create_test_event_id();
        let account_id1 = 1i64;
        let account_id2 = 999i64; // Non-existent account

        // Create a published event for account 1
        PublishedEvent::create(&event_id, account_id1, &database)
            .await
            .unwrap();

        // Check if it exists for account 2 (should be false)
        let exists = PublishedEvent::exists(&event_id, account_id2, &database)
            .await
            .unwrap();

        assert!(!exists);
    }

    #[tokio::test]
    async fn test_processed_event_create() {
        let pool = setup_test_db().await;
        let database = wrap_pool_in_database(pool);
        let event_id = create_test_event_id();
        let account_id = 1i64;

        // Create a processed event
        let result = ProcessedEvent::create(&event_id, account_id, &database).await;
        assert!(result.is_ok());

        // Verify it was inserted
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM processed_events WHERE event_id = ? AND account_id = ?",
        )
        .bind(event_id.to_hex())
        .bind(account_id)
        .fetch_one(&database.pool)
        .await
        .unwrap();

        assert_eq!(count.0, 1);
    }

    #[tokio::test]
    async fn test_processed_event_create_duplicate_ignored() {
        let pool = setup_test_db().await;
        let database = wrap_pool_in_database(pool);
        let event_id = create_test_event_id();
        let account_id = 1i64;

        // Create the same processed event twice
        let result1 = ProcessedEvent::create(&event_id, account_id, &database).await;
        let result2 = ProcessedEvent::create(&event_id, account_id, &database).await;

        assert!(result1.is_ok());
        assert!(result2.is_ok());

        // Verify only one record exists (INSERT OR IGNORE behavior)
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM processed_events WHERE event_id = ? AND account_id = ?",
        )
        .bind(event_id.to_hex())
        .bind(account_id)
        .fetch_one(&database.pool)
        .await
        .unwrap();

        assert_eq!(count.0, 1);
    }

    #[tokio::test]
    async fn test_processed_event_exists_true() {
        let pool = setup_test_db().await;
        let database = wrap_pool_in_database(pool);
        let event_id = create_test_event_id();
        let account_id = 1i64;

        // Create a processed event
        ProcessedEvent::create(&event_id, account_id, &database)
            .await
            .unwrap();

        // Check if it exists
        let exists = ProcessedEvent::exists(&event_id, account_id, &database)
            .await
            .unwrap();

        assert!(exists);
    }

    #[tokio::test]
    async fn test_processed_event_exists_false() {
        let pool = setup_test_db().await;
        let database = wrap_pool_in_database(pool);
        let event_id = create_test_event_id();
        let account_id = 1i64;

        // Check if non-existent event exists
        let exists = ProcessedEvent::exists(&event_id, account_id, &database)
            .await
            .unwrap();

        assert!(!exists);
    }

    #[tokio::test]
    async fn test_processed_event_exists_different_account() {
        let pool = setup_test_db().await;
        let database = wrap_pool_in_database(pool);
        let event_id = create_test_event_id();
        let account_id1 = 1i64;
        let account_id2 = 999i64; // Non-existent account

        // Create a processed event for account 1
        ProcessedEvent::create(&event_id, account_id1, &database)
            .await
            .unwrap();

        // Check if it exists for account 2 (should be false)
        let exists = ProcessedEvent::exists(&event_id, account_id2, &database)
            .await
            .unwrap();

        assert!(!exists);
    }

    #[tokio::test]
    async fn test_published_and_processed_events_independent() {
        let pool = setup_test_db().await;
        let database = wrap_pool_in_database(pool);
        let event_id = create_test_event_id();
        let account_id = 1i64;

        // Create only a published event
        PublishedEvent::create(&event_id, account_id, &database)
            .await
            .unwrap();

        // Check that published exists but processed doesn't
        let published_exists = PublishedEvent::exists(&event_id, account_id, &database)
            .await
            .unwrap();
        let processed_exists = ProcessedEvent::exists(&event_id, account_id, &database)
            .await
            .unwrap();

        assert!(published_exists);
        assert!(!processed_exists);

        // Now create processed event
        ProcessedEvent::create(&event_id, account_id, &database)
            .await
            .unwrap();

        // Check that both exist
        let published_exists = PublishedEvent::exists(&event_id, account_id, &database)
            .await
            .unwrap();
        let processed_exists = ProcessedEvent::exists(&event_id, account_id, &database)
            .await
            .unwrap();

        assert!(published_exists);
        assert!(processed_exists);
    }

    #[tokio::test]
    async fn test_multiple_accounts_same_event() {
        let pool = setup_test_db().await;
        let database = wrap_pool_in_database(pool);
        let event_id = create_test_event_id();

        // Create another test account
        sqlx::query(
            "INSERT INTO accounts (pubkey, user_id, created_at, updated_at)
             VALUES (?, 2, ?, ?)",
        )
        .bind("test_pubkey_2")
        .bind(Utc::now().timestamp())
        .bind(Utc::now().timestamp())
        .execute(&database.pool)
        .await
        .unwrap();

        let account_id1 = 1i64;
        let account_id2 = 2i64;

        // Both accounts can have records for the same event
        PublishedEvent::create(&event_id, account_id1, &database)
            .await
            .unwrap();
        PublishedEvent::create(&event_id, account_id2, &database)
            .await
            .unwrap();

        ProcessedEvent::create(&event_id, account_id1, &database)
            .await
            .unwrap();
        ProcessedEvent::create(&event_id, account_id2, &database)
            .await
            .unwrap();

        // Verify both accounts have their records
        assert!(PublishedEvent::exists(&event_id, account_id1, &database)
            .await
            .unwrap());
        assert!(PublishedEvent::exists(&event_id, account_id2, &database)
            .await
            .unwrap());
        assert!(ProcessedEvent::exists(&event_id, account_id1, &database)
            .await
            .unwrap());
        assert!(ProcessedEvent::exists(&event_id, account_id2, &database)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_published_event_struct_clone_debug_eq() {
        let event_id = create_test_event_id();
        let now = Utc::now();

        let event1 = PublishedEvent {
            id: 1,
            event_id,
            account_id: 123,
            created_at: now,
        };

        let event2 = event1.clone();
        assert_eq!(event1, event2);

        // Test Debug trait
        let debug_str = format!("{:?}", event1);
        assert!(debug_str.contains("PublishedEvent"));
        assert!(debug_str.contains(&event_id.to_hex()));
    }

    #[tokio::test]
    async fn test_processed_event_struct_clone_debug_eq() {
        let event_id = create_test_event_id();
        let now = Utc::now();

        let event1 = ProcessedEvent {
            id: 1,
            event_id,
            account_id: 123,
            processed_at: now,
        };

        let event2 = event1.clone();
        assert_eq!(event1, event2);

        // Test Debug trait
        let debug_str = format!("{:?}", event1);
        assert!(debug_str.contains("ProcessedEvent"));
        assert!(debug_str.contains(&event_id.to_hex()));
    }
}
