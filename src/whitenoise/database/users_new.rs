use super::DatabaseError;
use chrono::{DateTime, Utc};
use nostr_sdk::{Metadata, PublicKey};

#[allow(dead_code)]
#[derive(Debug)]
struct UserRow {
    // id is the primary key
    id: i64,
    // pubkey is the hex encoded nostr public key
    pubkey: PublicKey,
    // metadata is the JSONB column that stores the user metadata
    metadata: Metadata,
    // created_at is the timestamp of the user creation
    created_at: DateTime<Utc>,
    // updated_at is the timestamp of the last update
    updated_at: DateTime<Utc>,
}

impl<'r, R> sqlx::FromRow<'r, R> for UserRow
where
    R: sqlx::Row,
    &'r str: sqlx::ColumnIndex<R>,
    String: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
    i64: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
{
    fn from_row(row: &'r R) -> std::result::Result<Self, sqlx::Error> {
        // Extract raw values from the database row
        let id: i64 = row.try_get("id")?;
        let pubkey_str: String = row.try_get("pubkey")?;
        let metadata_json: String = row.try_get("metadata")?;
        let created_at_i64: i64 = row.try_get("created_at")?;
        let updated_at_i64: i64 = row.try_get("updated_at")?;

        // Parse pubkey from hex string
        let pubkey = PublicKey::parse(&pubkey_str).map_err(|e| sqlx::Error::ColumnDecode {
            index: "pubkey".to_string(),
            source: Box::new(e),
        })?;

        // Parse metadata from JSON
        let metadata: Metadata =
            serde_json::from_str(&metadata_json).map_err(|e| sqlx::Error::ColumnDecode {
                index: "metadata".to_string(),
                source: Box::new(e),
            })?;

        let created_at = DateTime::from_timestamp_millis(created_at_i64)
            .ok_or_else(|| DatabaseError::InvalidTimestamp {
                timestamp: created_at_i64,
            })
            .map_err(|e| sqlx::Error::ColumnDecode {
                index: "created_at".to_string(),
                source: Box::new(e),
            })?;

        let updated_at = DateTime::from_timestamp_millis(updated_at_i64)
            .ok_or_else(|| DatabaseError::InvalidTimestamp {
                timestamp: updated_at_i64,
            })
            .map_err(|e| sqlx::Error::ColumnDecode {
                index: "updated_at".to_string(),
                source: Box::new(e),
            })?;

        Ok(UserRow {
            id,
            pubkey,
            metadata,
            created_at,
            updated_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqliteRow;
    use sqlx::{FromRow, SqlitePool};

    // Helper function to create a mock SQLite row
    async fn setup_test_db() -> SqlitePool {
        let pool = SqlitePool::connect(":memory:").await.unwrap();

        // Create the users table with the new schema
        sqlx::query(
            "CREATE TABLE users_new (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                pubkey TEXT NOT NULL,
                metadata JSONB,
                created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    #[tokio::test]
    async fn test_user_row_from_row_valid_data() {
        let pool = setup_test_db().await;

        // Create valid test data
        let test_pubkey = nostr_sdk::Keys::generate().public_key();
        let test_metadata = Metadata::new()
            .name("Test User")
            .display_name("Test Display Name")
            .about("Test about section");
        let test_metadata_json = serde_json::to_string(&test_metadata).unwrap();
        let test_timestamp = chrono::Utc::now().timestamp_millis();

        // Insert test data
        sqlx::query(
            "INSERT INTO users_new (pubkey, metadata, created_at, updated_at) VALUES (?, ?, ?, ?)",
        )
        .bind(test_pubkey.to_hex())
        .bind(test_metadata_json)
        .bind(test_timestamp)
        .bind(test_timestamp)
        .execute(&pool)
        .await
        .unwrap();

        // Test from_row implementation
        let row: SqliteRow = sqlx::query("SELECT * FROM users_new WHERE pubkey = ?")
            .bind(test_pubkey.to_hex())
            .fetch_one(&pool)
            .await
            .unwrap();

        let user_row = UserRow::from_row(&row).unwrap();

        assert_eq!(user_row.pubkey, test_pubkey);
        assert_eq!(user_row.metadata.name, test_metadata.name);
        assert_eq!(user_row.metadata.display_name, test_metadata.display_name);
        assert_eq!(user_row.metadata.about, test_metadata.about);
        assert_eq!(user_row.created_at.timestamp_millis(), test_timestamp);
        assert_eq!(user_row.updated_at.timestamp_millis(), test_timestamp);
    }

    #[tokio::test]
    async fn test_user_row_from_row_minimal_metadata() {
        let pool = setup_test_db().await;

        let test_pubkey = nostr_sdk::Keys::generate().public_key();
        let test_metadata = Metadata::new(); // Minimal metadata
        let test_metadata_json = serde_json::to_string(&test_metadata).unwrap();
        let test_timestamp = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            "INSERT INTO users_new (pubkey, metadata, created_at, updated_at) VALUES (?, ?, ?, ?)",
        )
        .bind(test_pubkey.to_hex())
        .bind(test_metadata_json)
        .bind(test_timestamp)
        .bind(test_timestamp)
        .execute(&pool)
        .await
        .unwrap();

        let row: SqliteRow = sqlx::query("SELECT * FROM users_new WHERE pubkey = ?")
            .bind(test_pubkey.to_hex())
            .fetch_one(&pool)
            .await
            .unwrap();

        let user_row = UserRow::from_row(&row).unwrap();
        assert_eq!(user_row.pubkey, test_pubkey);
        assert_eq!(user_row.metadata.name, None);
    }

    #[tokio::test]
    async fn test_user_row_from_row_invalid_pubkey() {
        let pool = setup_test_db().await;

        let invalid_pubkey = "invalid_hex_key";
        let test_metadata = Metadata::new();
        let test_metadata_json = serde_json::to_string(&test_metadata).unwrap();
        let test_timestamp = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            "INSERT INTO users_new (pubkey, metadata, created_at, updated_at) VALUES (?, ?, ?, ?)",
        )
        .bind(invalid_pubkey)
        .bind(test_metadata_json)
        .bind(test_timestamp)
        .bind(test_timestamp)
        .execute(&pool)
        .await
        .unwrap();

        let row: SqliteRow = sqlx::query("SELECT * FROM users_new WHERE pubkey = ?")
            .bind(invalid_pubkey)
            .fetch_one(&pool)
            .await
            .unwrap();

        let result = UserRow::from_row(&row);
        assert!(result.is_err());

        if let Err(sqlx::Error::ColumnDecode { index, .. }) = result {
            assert_eq!(index, "pubkey");
        } else {
            panic!("Expected ColumnDecode error for pubkey");
        }
    }

    #[tokio::test]
    async fn test_user_row_from_row_invalid_metadata_json() {
        let pool = setup_test_db().await;

        let test_pubkey = nostr_sdk::Keys::generate().public_key();
        let invalid_json = "{invalid_json}";
        let test_timestamp = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            "INSERT INTO users_new (pubkey, metadata, created_at, updated_at) VALUES (?, ?, ?, ?)",
        )
        .bind(test_pubkey.to_hex())
        .bind(invalid_json)
        .bind(test_timestamp)
        .bind(test_timestamp)
        .execute(&pool)
        .await
        .unwrap();

        let row: SqliteRow = sqlx::query("SELECT * FROM users_new WHERE pubkey = ?")
            .bind(test_pubkey.to_hex())
            .fetch_one(&pool)
            .await
            .unwrap();

        let result = UserRow::from_row(&row);
        assert!(result.is_err());

        if let Err(sqlx::Error::ColumnDecode { index, .. }) = result {
            assert_eq!(index, "metadata");
        } else {
            panic!("Expected ColumnDecode error for metadata");
        }
    }

    #[tokio::test]
    async fn test_user_row_from_row_timestamp_edge_cases() {
        let pool = setup_test_db().await;

        let test_pubkey = nostr_sdk::Keys::generate().public_key();
        let test_metadata = Metadata::new();
        let test_metadata_json = serde_json::to_string(&test_metadata).unwrap();

        // Test with timestamp 0 (Unix epoch)
        sqlx::query(
            "INSERT INTO users_new (pubkey, metadata, created_at, updated_at) VALUES (?, ?, ?, ?)",
        )
        .bind(test_pubkey.to_hex())
        .bind(&test_metadata_json)
        .bind(0i64)
        .bind(0i64)
        .execute(&pool)
        .await
        .unwrap();

        let row: SqliteRow = sqlx::query("SELECT * FROM users_new WHERE pubkey = ?")
            .bind(test_pubkey.to_hex())
            .fetch_one(&pool)
            .await
            .unwrap();

        let user_row = UserRow::from_row(&row).unwrap();
        assert_eq!(user_row.created_at.timestamp_millis(), 0);
        assert_eq!(user_row.updated_at.timestamp_millis(), 0);
    }

    #[tokio::test]
    async fn test_user_row_from_row_invalid_timestamps() {
        let pool = setup_test_db().await;

        let test_pubkey = nostr_sdk::Keys::generate().public_key();
        let test_metadata = Metadata::new();
        let test_metadata_json = serde_json::to_string(&test_metadata).unwrap();

        // Test with invalid timestamp that's out of range for DateTime
        let invalid_timestamp = i64::MAX; // This will be too large for DateTime conversion

        sqlx::query(
            "INSERT INTO users_new (pubkey, metadata, created_at, updated_at) VALUES (?, ?, ?, ?)",
        )
        .bind(test_pubkey.to_hex())
        .bind(&test_metadata_json)
        .bind(invalid_timestamp)
        .bind(0i64) // Valid timestamp for updated_at
        .execute(&pool)
        .await
        .unwrap();

        let row: SqliteRow = sqlx::query("SELECT * FROM users_new WHERE pubkey = ?")
            .bind(test_pubkey.to_hex())
            .fetch_one(&pool)
            .await
            .unwrap();

        let result = UserRow::from_row(&row);
        assert!(result.is_err());

        if let Err(sqlx::Error::ColumnDecode { index, .. }) = result {
            assert_eq!(index, "created_at");
        } else {
            panic!("Expected ColumnDecode error for created_at timestamp");
        }

        // Clean up and test invalid updated_at timestamp
        sqlx::query("DELETE FROM users_new WHERE pubkey = ?")
            .bind(test_pubkey.to_hex())
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO users_new (pubkey, metadata, created_at, updated_at) VALUES (?, ?, ?, ?)",
        )
        .bind(test_pubkey.to_hex())
        .bind(&test_metadata_json)
        .bind(0i64) // Valid timestamp for created_at
        .bind(invalid_timestamp)
        .execute(&pool)
        .await
        .unwrap();

        let row: SqliteRow = sqlx::query("SELECT * FROM users_new WHERE pubkey = ?")
            .bind(test_pubkey.to_hex())
            .fetch_one(&pool)
            .await
            .unwrap();

        let result = UserRow::from_row(&row);
        assert!(result.is_err());

        if let Err(sqlx::Error::ColumnDecode { index, .. }) = result {
            assert_eq!(index, "updated_at");
        } else {
            panic!("Expected ColumnDecode error for updated_at timestamp");
        }
    }
}
