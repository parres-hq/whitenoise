use super::DatabaseError;
use crate::whitenoise::relays::Relay;
use crate::{Whitenoise, WhitenoiseError};
use chrono::{DateTime, Utc};
use nostr_sdk::RelayUrl;

#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct RelayRow {
    // id is the primary key
    pub id: i64,
    // url is the URL of the relay
    pub url: RelayUrl,
    // created_at is the timestamp of the relay creation
    pub created_at: DateTime<Utc>,
    // updated_at is the timestamp of the last update
    pub updated_at: DateTime<Utc>,
}

impl<'r, R> sqlx::FromRow<'r, R> for RelayRow
where
    R: sqlx::Row,
    &'r str: sqlx::ColumnIndex<R>,
    String: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
    i64: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
{
    fn from_row(row: &'r R) -> std::result::Result<Self, sqlx::Error> {
        // Extract raw values from the database row
        let id: i64 = row.try_get("id")?;
        let url_str: String = row.try_get("url")?;
        let created_at_i64: i64 = row.try_get("created_at")?;
        let updated_at_i64: i64 = row.try_get("updated_at")?;

        // Parse url from string
        let url = RelayUrl::parse(&url_str).map_err(|e| sqlx::Error::ColumnDecode {
            index: "url".to_string(),
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

        Ok(RelayRow {
            id,
            url,
            created_at,
            updated_at,
        })
    }
}

impl Whitenoise {
    #[allow(dead_code)]
    pub(crate) async fn load_relay(&self, url: &RelayUrl) -> Result<Relay, WhitenoiseError> {
        let relay_row = sqlx::query_as::<_, RelayRow>("SELECT * FROM relays WHERE url = ?")
            .bind(url.to_string().as_str())
            .fetch_one(&self.database.pool)
            .await
            .map_err(|_| WhitenoiseError::RelayNotFound)?;

        Ok(Relay {
            id: relay_row.id,
            url: relay_row.url,
            created_at: relay_row.created_at,
            updated_at: relay_row.updated_at,
        })
    }

    #[allow(dead_code)]
    pub(crate) async fn save_relay(&self, relay: &Relay) -> Result<(), DatabaseError> {
        sqlx::query("INSERT INTO relays (url, created_at, updated_at) VALUES (?, ?, ?)")
            .bind(relay.url.to_string().as_str())
            .bind(relay.created_at.timestamp_millis())
            .bind(relay.updated_at.timestamp_millis())
            .execute(&self.database.pool)
            .await
            .map_err(DatabaseError::Sqlx)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqliteRow;
    use sqlx::{FromRow, SqlitePool};

    async fn setup_test_db() -> SqlitePool {
        let pool = SqlitePool::connect(":memory:").await.unwrap();

        // Create the relays table
        sqlx::query(
            "CREATE TABLE relays (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT NOT NULL,
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
    async fn test_relay_new_row_from_row_valid_data() {
        let pool = setup_test_db().await;

        let test_url_str = "wss://relay.damus.io";
        let test_url = RelayUrl::parse(test_url_str).unwrap();
        let test_timestamp = chrono::Utc::now().timestamp_millis();

        sqlx::query("INSERT INTO relays (url, created_at, updated_at) VALUES (?, ?, ?)")
            .bind(test_url_str)
            .bind(test_timestamp)
            .bind(test_timestamp)
            .execute(&pool)
            .await
            .unwrap();

        let row: SqliteRow = sqlx::query("SELECT * FROM relays WHERE url = ?")
            .bind(test_url_str)
            .fetch_one(&pool)
            .await
            .unwrap();

        let relay_row = RelayRow::from_row(&row).unwrap();

        assert_eq!(relay_row.url, test_url);
        assert_eq!(relay_row.created_at.timestamp_millis(), test_timestamp);
        assert_eq!(relay_row.updated_at.timestamp_millis(), test_timestamp);
    }

    #[tokio::test]
    async fn test_relay_new_row_from_row_various_valid_urls() {
        let pool = setup_test_db().await;

        let test_urls = vec![
            "wss://relay.damus.io",
            "wss://nos.lol",
            "wss://relay.snort.social",
            "wss://relay.nostr.band",
            "ws://localhost:8080", // Non-secure websocket for testing
        ];

        let test_timestamp = chrono::Utc::now().timestamp_millis();

        for url_str in test_urls {
            // Insert the URL
            sqlx::query("INSERT INTO relays (url, created_at, updated_at) VALUES (?, ?, ?)")
                .bind(url_str)
                .bind(test_timestamp)
                .bind(test_timestamp)
                .execute(&pool)
                .await
                .unwrap();

            // Fetch and test
            let row: SqliteRow = sqlx::query("SELECT * FROM relays WHERE url = ?")
                .bind(url_str)
                .fetch_one(&pool)
                .await
                .unwrap();

            let relay_row = RelayRow::from_row(&row).unwrap();
            let expected_url = RelayUrl::parse(url_str).unwrap();
            assert_eq!(relay_row.url, expected_url);

            // Clean up
            sqlx::query("DELETE FROM relays WHERE url = ?")
                .bind(url_str)
                .execute(&pool)
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn test_relay_new_row_from_row_invalid_url() {
        let pool = setup_test_db().await;

        let invalid_urls = vec![
            "not_a_url",
            "http://invalid_for_relay.com", // HTTP instead of WS/WSS
            "ftp://not.websocket.com",
            "",
            "just some text",
            "wss://", // Incomplete URL
        ];

        let test_timestamp = chrono::Utc::now().timestamp_millis();

        for invalid_url in invalid_urls {
            // Insert invalid URL (SQLite will accept it)
            sqlx::query("INSERT INTO relays (url, created_at, updated_at) VALUES (?, ?, ?)")
                .bind(invalid_url)
                .bind(test_timestamp)
                .bind(test_timestamp)
                .execute(&pool)
                .await
                .unwrap();

            // Try to parse it with from_row - should fail
            let row: SqliteRow = sqlx::query("SELECT * FROM relays WHERE url = ?")
                .bind(invalid_url)
                .fetch_one(&pool)
                .await
                .unwrap();

            let result = RelayRow::from_row(&row);
            assert!(
                result.is_err(),
                "Expected error for invalid URL: {}",
                invalid_url
            );

            if let Err(sqlx::Error::ColumnDecode { index, .. }) = result {
                assert_eq!(index, "url");
            } else {
                panic!("Expected ColumnDecode error for url, got: {:?}", result);
            }

            // Clean up
            sqlx::query("DELETE FROM relays WHERE url = ?")
                .bind(invalid_url)
                .execute(&pool)
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn test_relay_new_row_from_row_timestamp_edge_cases() {
        let pool = setup_test_db().await;

        let test_url = "wss://relay.damus.io";

        // Test with timestamp 0 (Unix epoch)
        sqlx::query("INSERT INTO relays (url, created_at, updated_at) VALUES (?, ?, ?)")
            .bind(test_url)
            .bind(0i64)
            .bind(0i64)
            .execute(&pool)
            .await
            .unwrap();

        let row: SqliteRow = sqlx::query("SELECT * FROM relays WHERE created_at = 0")
            .fetch_one(&pool)
            .await
            .unwrap();

        let relay_row = RelayRow::from_row(&row).unwrap();
        assert_eq!(relay_row.created_at.timestamp_millis(), 0);
        assert_eq!(relay_row.updated_at.timestamp_millis(), 0);

        // Clean up
        sqlx::query("DELETE FROM relays WHERE created_at = 0")
            .execute(&pool)
            .await
            .unwrap();

        // Test with future timestamp
        let future_timestamp =
            (chrono::Utc::now() + chrono::Duration::days(365)).timestamp_millis();
        sqlx::query("INSERT INTO relays (url, created_at, updated_at) VALUES (?, ?, ?)")
            .bind(test_url)
            .bind(future_timestamp)
            .bind(future_timestamp)
            .execute(&pool)
            .await
            .unwrap();

        let row: SqliteRow = sqlx::query("SELECT * FROM relays WHERE created_at = ?")
            .bind(future_timestamp)
            .fetch_one(&pool)
            .await
            .unwrap();

        let relay_row = RelayRow::from_row(&row).unwrap();
        assert_eq!(relay_row.created_at.timestamp_millis(), future_timestamp);
        assert_eq!(relay_row.updated_at.timestamp_millis(), future_timestamp);
    }

    #[tokio::test]
    async fn test_relay_new_row_from_row_url_with_port() {
        let pool = setup_test_db().await;

        let test_url_str = "wss://relay.example.com:8443";
        let test_timestamp = chrono::Utc::now().timestamp_millis();

        sqlx::query("INSERT INTO relays (url, created_at, updated_at) VALUES (?, ?, ?)")
            .bind(test_url_str)
            .bind(test_timestamp)
            .bind(test_timestamp)
            .execute(&pool)
            .await
            .unwrap();

        let row: SqliteRow = sqlx::query("SELECT * FROM relays WHERE url = ?")
            .bind(test_url_str)
            .fetch_one(&pool)
            .await
            .unwrap();

        let relay_row = RelayRow::from_row(&row).unwrap();
        let expected_url = RelayUrl::parse(test_url_str).unwrap();
        assert_eq!(relay_row.url, expected_url);
    }

    #[tokio::test]
    async fn test_relay_new_row_from_row_url_with_path() {
        let pool = setup_test_db().await;

        let test_url_str = "wss://relay.example.com/nostr";
        let test_timestamp = chrono::Utc::now().timestamp_millis();

        sqlx::query("INSERT INTO relays (url, created_at, updated_at) VALUES (?, ?, ?)")
            .bind(test_url_str)
            .bind(test_timestamp)
            .bind(test_timestamp)
            .execute(&pool)
            .await
            .unwrap();

        let row: SqliteRow = sqlx::query("SELECT * FROM relays WHERE url = ?")
            .bind(test_url_str)
            .fetch_one(&pool)
            .await
            .unwrap();

        let relay_row = RelayRow::from_row(&row).unwrap();
        let expected_url = RelayUrl::parse(test_url_str).unwrap();
        assert_eq!(relay_row.url, expected_url);
    }

    #[tokio::test]
    async fn test_relay_new_row_from_row_invalid_timestamps() {
        let pool = setup_test_db().await;

        let test_url = "wss://relay.damus.io";
        let valid_timestamp = chrono::Utc::now().timestamp_millis();
        let invalid_timestamp = i64::MAX; // This will be too large for DateTime conversion

        // Test invalid created_at timestamp
        sqlx::query("INSERT INTO relays (url, created_at, updated_at) VALUES (?, ?, ?)")
            .bind(test_url)
            .bind(invalid_timestamp)
            .bind(valid_timestamp)
            .execute(&pool)
            .await
            .unwrap();

        let row: SqliteRow = sqlx::query("SELECT * FROM relays WHERE url = ?")
            .bind(test_url)
            .fetch_one(&pool)
            .await
            .unwrap();

        let result = RelayRow::from_row(&row);
        assert!(result.is_err());

        if let Err(sqlx::Error::ColumnDecode { index, .. }) = result {
            assert_eq!(index, "created_at");
        } else {
            panic!("Expected ColumnDecode error for created_at timestamp");
        }

        // Clean up and test invalid updated_at timestamp
        sqlx::query("DELETE FROM relays WHERE url = ?")
            .bind(test_url)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query("INSERT INTO relays (url, created_at, updated_at) VALUES (?, ?, ?)")
            .bind(test_url)
            .bind(valid_timestamp)
            .bind(invalid_timestamp)
            .execute(&pool)
            .await
            .unwrap();

        let row: SqliteRow = sqlx::query("SELECT * FROM relays WHERE url = ?")
            .bind(test_url)
            .fetch_one(&pool)
            .await
            .unwrap();

        let result = RelayRow::from_row(&row);
        assert!(result.is_err());

        if let Err(sqlx::Error::ColumnDecode { index, .. }) = result {
            assert_eq!(index, "updated_at");
        } else {
            panic!("Expected ColumnDecode error for updated_at timestamp");
        }
    }
}
