use chrono::{DateTime, Utc};
use nostr_sdk::{EventId, Kind};

use super::{Database, DatabaseError};

/// Row structure for published_events table
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct PublishedEvent {
    pub id: i64,
    pub event_id: EventId,
    pub account_id: i64,
    pub event_kind: Kind,
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
        let event_kind_num: i64 = row.try_get("event_kind")?;
        let created_at_timestamp: i64 = row.try_get("created_at")?;

        let event_id =
            EventId::from_hex(&event_id_hex).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        let event_kind = Kind::from(event_kind_num as u16);

        let created_at = DateTime::from_timestamp(created_at_timestamp, 0)
            .ok_or_else(|| sqlx::Error::Decode("Invalid timestamp".into()))?;

        Ok(PublishedEvent {
            id,
            event_id,
            account_id,
            event_kind,
            created_at,
        })
    }
}

/// Row structure for processed_events table
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ProcessedEvent {
    pub id: i64,
    pub event_id: EventId,
    pub event_kind: Kind,
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
        let event_kind_num: i64 = row.try_get("event_kind")?;
        let processed_at_timestamp: i64 = row.try_get("processed_at")?;

        let event_id =
            EventId::from_hex(&event_id_hex).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        let event_kind = Kind::from(event_kind_num as u16);

        let processed_at = DateTime::from_timestamp(processed_at_timestamp, 0)
            .ok_or_else(|| sqlx::Error::Decode("Invalid timestamp".into()))?;

        Ok(ProcessedEvent {
            id,
            event_id,
            event_kind,
            processed_at,
        })
    }
}

impl PublishedEvent {
    /// Records that we published a specific event to prevent processing our own events
    pub async fn create(
        database: &Database,
        event_id: &EventId,
        account_id: i64,
        event_kind: Kind,
    ) -> Result<(), DatabaseError> {
        sqlx::query(
            "INSERT INTO published_events (event_id, account_id, event_kind) VALUES (?, ?, ?)",
        )
        .bind(event_id.to_hex())
        .bind(account_id)
        .bind(event_kind.as_u16() as i64)
        .execute(&database.pool)
        .await?;

        tracing::debug!(
            target: "whitenoise::database::published_events::create",
            "Recorded published event: {} (kind {}) by account ID {}",
            event_id.to_hex(),
            event_kind.as_u16(),
            account_id
        );

        Ok(())
    }

    /// Checks if we published a specific event
    pub async fn exists(
        database: &Database,
        event_id: &EventId,
        account_id: i64,
    ) -> Result<bool, DatabaseError> {
        let result: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM published_events WHERE event_id = ? AND account_id = ?",
        )
        .bind(event_id.to_hex())
        .bind(account_id)
        .fetch_one(&database.pool)
        .await?;

        Ok(result.0 > 0)
    }

    /// Gets all published events for a specific account and event kind
    pub async fn find_by_account_and_kind(
        database: &Database,
        account_id: i64,
        event_kind: Kind,
    ) -> Result<Vec<PublishedEvent>, DatabaseError> {
        let rows = sqlx::query_as::<_, PublishedEvent>(
            "SELECT id, event_id, account_id, event_kind,
             CAST(strftime('%s', created_at) AS INTEGER) as created_at
             FROM published_events
             WHERE account_id = ? AND event_kind = ?
             ORDER BY created_at DESC",
        )
        .bind(account_id)
        .bind(event_kind.as_u16() as i64)
        .fetch_all(&database.pool)
        .await?;

        Ok(rows)
    }

    /// Cleans up old published events (older than specified days)
    pub async fn cleanup_old(database: &Database, days_old: i32) -> Result<u64, DatabaseError> {
        let result = sqlx::query(
            "DELETE FROM published_events WHERE created_at < datetime('now', '-' || ? || ' days')",
        )
        .bind(days_old)
        .execute(&database.pool)
        .await?;

        Ok(result.rows_affected())
    }
}

impl ProcessedEvent {
    /// Records that we processed a specific event to ensure idempotency
    pub async fn create(
        database: &Database,
        event_id: &EventId,
        event_kind: Kind,
    ) -> Result<(), DatabaseError> {
        // Use INSERT OR IGNORE to handle potential race conditions
        sqlx::query("INSERT OR IGNORE INTO processed_events (event_id, event_kind) VALUES (?, ?)")
            .bind(event_id.to_hex())
            .bind(event_kind.as_u16() as i64)
            .execute(&database.pool)
            .await?;

        Ok(())
    }

    /// Checks if we already processed a specific event
    pub async fn exists(database: &Database, event_id: &EventId) -> Result<bool, DatabaseError> {
        let result: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM processed_events WHERE event_id = ?")
                .bind(event_id.to_hex())
                .fetch_one(&database.pool)
                .await?;

        Ok(result.0 > 0)
    }

    /// Gets all processed events for a specific event kind
    pub async fn find_by_kind(
        database: &Database,
        event_kind: Kind,
    ) -> Result<Vec<ProcessedEvent>, DatabaseError> {
        let rows = sqlx::query_as::<_, ProcessedEvent>(
            "SELECT id, event_id, event_kind,
             CAST(strftime('%s', processed_at) AS INTEGER) as processed_at
             FROM processed_events
             WHERE event_kind = ?
             ORDER BY processed_at DESC",
        )
        .bind(event_kind.as_u16() as i64)
        .fetch_all(&database.pool)
        .await?;

        Ok(rows)
    }

    /// Cleans up old processed events (older than specified days)
    pub async fn cleanup_old(database: &Database, days_old: i32) -> Result<u64, DatabaseError> {
        let result = sqlx::query(
            "DELETE FROM processed_events WHERE processed_at < datetime('now', '-' || ? || ' days')"
        )
        .bind(days_old)
        .execute(&database.pool)
        .await?;

        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::whitenoise::database::Database;
    use nostr_sdk::Keys;
    use tempfile::TempDir;

    async fn create_test_db() -> (Database, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let db_path = temp_dir.path().join("test.db");
        let db = Database::new(db_path)
            .await
            .expect("Failed to create test database");
        (db, temp_dir)
    }

    async fn create_test_account(db: &Database) -> (i64, i64) {
        let keys = Keys::generate();
        let pubkey = keys.public_key();

        // Create user first
        sqlx::query("INSERT INTO users (pubkey, metadata) VALUES (?, '{}')")
            .bind(pubkey.to_hex())
            .execute(&db.pool)
            .await
            .expect("Failed to insert test user");

        let (user_id,): (i64,) = sqlx::query_as("SELECT id FROM users WHERE pubkey = ?")
            .bind(pubkey.to_hex())
            .fetch_one(&db.pool)
            .await
            .expect("Failed to get user ID");

        // Create account
        sqlx::query("INSERT INTO accounts (pubkey, user_id) VALUES (?, ?)")
            .bind(pubkey.to_hex())
            .bind(user_id)
            .execute(&db.pool)
            .await
            .expect("Failed to insert test account");

        let (account_id,): (i64,) = sqlx::query_as("SELECT id FROM accounts WHERE pubkey = ?")
            .bind(pubkey.to_hex())
            .fetch_one(&db.pool)
            .await
            .expect("Failed to get account ID");

        (account_id, user_id)
    }

    #[tokio::test]
    async fn test_record_and_check_published_event() {
        let (db, _temp_dir) = create_test_db().await;
        let (account_id, _user_id) = create_test_account(&db).await;

        let event_id =
            EventId::from_hex("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
                .unwrap();
        let event_kind = Kind::ContactList;

        // Initially should not exist
        let exists = PublishedEvent::exists(&db, &event_id, account_id)
            .await
            .unwrap();
        assert!(!exists);

        // Record the event
        PublishedEvent::create(&db, &event_id, account_id, event_kind)
            .await
            .unwrap();

        // Now should exist
        let exists = PublishedEvent::exists(&db, &event_id, account_id)
            .await
            .unwrap();
        assert!(exists);
    }

    #[tokio::test]
    async fn test_record_and_check_processed_event() {
        let (db, _temp_dir) = create_test_db().await;

        let event_id =
            EventId::from_hex("fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210")
                .unwrap();
        let event_kind = Kind::Metadata;

        // Initially should not exist
        let exists = ProcessedEvent::exists(&db, &event_id).await.unwrap();
        assert!(!exists);

        // Record the event
        ProcessedEvent::create(&db, &event_id, event_kind)
            .await
            .unwrap();

        // Now should exist
        let exists = ProcessedEvent::exists(&db, &event_id).await.unwrap();
        assert!(exists);
    }

    #[tokio::test]
    async fn test_idempotent_processed_event_insert() {
        let (db, _temp_dir) = create_test_db().await;

        let event_id =
            EventId::from_hex("abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789")
                .unwrap();
        let event_kind = Kind::RelayList;

        // Record the same event twice - should not error
        ProcessedEvent::create(&db, &event_id, event_kind)
            .await
            .unwrap();
        ProcessedEvent::create(&db, &event_id, event_kind)
            .await
            .unwrap();

        // Should still exist only once
        let exists = ProcessedEvent::exists(&db, &event_id).await.unwrap();
        assert!(exists);
    }

    #[tokio::test]
    async fn test_get_published_events_by_account_and_kind() {
        let (db, _temp_dir) = create_test_db().await;
        let (account_id, _user_id) = create_test_account(&db).await;

        let event_id1 =
            EventId::from_hex("1111111111111111111111111111111111111111111111111111111111111111")
                .unwrap();
        let event_id2 =
            EventId::from_hex("2222222222222222222222222222222222222222222222222222222222222222")
                .unwrap();
        let event_kind = Kind::ContactList;

        // Record two events
        PublishedEvent::create(&db, &event_id1, account_id, event_kind)
            .await
            .unwrap();
        PublishedEvent::create(&db, &event_id2, account_id, event_kind)
            .await
            .unwrap();

        // Get all events for this account and kind
        let events = PublishedEvent::find_by_account_and_kind(&db, account_id, event_kind)
            .await
            .unwrap();

        assert_eq!(events.len(), 2);
        assert!(events.iter().any(|e| e.event_id == event_id1));
        assert!(events.iter().any(|e| e.event_id == event_id2));
    }
}
