use super::{relays::RelayRow, DatabaseError};
use crate::whitenoise::relays::{Relay, RelayType};
use crate::whitenoise::users::User;
use crate::whitenoise::{Whitenoise, WhitenoiseError};
use chrono::{DateTime, Utc};
use nostr_sdk::{Metadata, PublicKey};

#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct UserRow {
    // id is the primary key
    pub id: i64,
    // pubkey is the hex encoded nostr public key
    pub pubkey: PublicKey,
    // metadata is the JSONB column that stores the user metadata
    pub metadata: Metadata,
    // created_at is the timestamp of the user creation
    pub created_at: DateTime<Utc>,
    // updated_at is the timestamp of the last update
    pub updated_at: DateTime<Utc>,
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

impl Whitenoise {
    #[allow(dead_code)]
    pub(crate) async fn load_user(&self, pubkey: &PublicKey) -> Result<User, WhitenoiseError> {
        let user_row = sqlx::query_as::<_, UserRow>("SELECT * FROM users WHERE pubkey = ?")
            .bind(pubkey.to_hex().as_str())
            .fetch_one(&self.database.pool)
            .await
            .map_err(|_| WhitenoiseError::UserNotFound)?;

        Ok(User {
            id: user_row.id,
            pubkey: user_row.pubkey,
            metadata: user_row.metadata,
            created_at: user_row.created_at,
            updated_at: user_row.updated_at,
        })
    }

    #[allow(dead_code)]
    pub(crate) async fn load_user_relays(
        &self,
        user: &User,
        relay_type: RelayType,
    ) -> Result<Vec<Relay>, WhitenoiseError> {
        let relay_type_str = String::from(relay_type);

        let relay_rows = sqlx::query_as::<_, RelayRow>(
            "SELECT r.id, r.url, r.created_at, r.updated_at
             FROM relays r
             INNER JOIN user_relays ur ON r.id = ur.relay_id
             WHERE ur.user_id = ? AND ur.relay_type = ?",
        )
        .bind(user.id)
        .bind(relay_type_str)
        .fetch_all(&self.database.pool)
        .await
        .map_err(DatabaseError::Sqlx)
        .map_err(WhitenoiseError::Database)?;

        let relays = relay_rows
            .into_iter()
            .map(|row| Relay {
                id: row.id,
                url: row.url,
                created_at: row.created_at,
                updated_at: row.updated_at,
            })
            .collect();

        Ok(relays)
    }

    #[allow(dead_code)]
    pub(crate) async fn save_user(&self, user: &User) -> Result<(), DatabaseError> {
        sqlx::query(
            "INSERT INTO users (pubkey, metadata, created_at, updated_at) VALUES (?, ?, ?, ?)",
        )
        .bind(user.pubkey.to_hex().as_str())
        .bind(serde_json::to_string(&user.metadata).unwrap())
        .bind(user.created_at.timestamp_millis())
        .bind(user.updated_at.timestamp_millis())
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

    // Helper function to create a mock SQLite row
    async fn setup_test_db() -> SqlitePool {
        let pool = SqlitePool::connect(":memory:").await.unwrap();

        // Create the users table with the new schema
        sqlx::query(
            "CREATE TABLE users (
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
            "INSERT INTO users (pubkey, metadata, created_at, updated_at) VALUES (?, ?, ?, ?)",
        )
        .bind(test_pubkey.to_hex())
        .bind(test_metadata_json)
        .bind(test_timestamp)
        .bind(test_timestamp)
        .execute(&pool)
        .await
        .unwrap();

        // Test from_row implementation
        let row: SqliteRow = sqlx::query("SELECT * FROM users WHERE pubkey = ?")
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
            "INSERT INTO users (pubkey, metadata, created_at, updated_at) VALUES (?, ?, ?, ?)",
        )
        .bind(test_pubkey.to_hex())
        .bind(test_metadata_json)
        .bind(test_timestamp)
        .bind(test_timestamp)
        .execute(&pool)
        .await
        .unwrap();

        let row: SqliteRow = sqlx::query("SELECT * FROM users WHERE pubkey = ?")
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
            "INSERT INTO users (pubkey, metadata, created_at, updated_at) VALUES (?, ?, ?, ?)",
        )
        .bind(invalid_pubkey)
        .bind(test_metadata_json)
        .bind(test_timestamp)
        .bind(test_timestamp)
        .execute(&pool)
        .await
        .unwrap();

        let row: SqliteRow = sqlx::query("SELECT * FROM users WHERE pubkey = ?")
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
            "INSERT INTO users (pubkey, metadata, created_at, updated_at) VALUES (?, ?, ?, ?)",
        )
        .bind(test_pubkey.to_hex())
        .bind(invalid_json)
        .bind(test_timestamp)
        .bind(test_timestamp)
        .execute(&pool)
        .await
        .unwrap();

        let row: SqliteRow = sqlx::query("SELECT * FROM users WHERE pubkey = ?")
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
            "INSERT INTO users (pubkey, metadata, created_at, updated_at) VALUES (?, ?, ?, ?)",
        )
        .bind(test_pubkey.to_hex())
        .bind(&test_metadata_json)
        .bind(0i64)
        .bind(0i64)
        .execute(&pool)
        .await
        .unwrap();

        let row: SqliteRow = sqlx::query("SELECT * FROM users WHERE pubkey = ?")
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
            "INSERT INTO users (pubkey, metadata, created_at, updated_at) VALUES (?, ?, ?, ?)",
        )
        .bind(test_pubkey.to_hex())
        .bind(&test_metadata_json)
        .bind(invalid_timestamp)
        .bind(0i64) // Valid timestamp for updated_at
        .execute(&pool)
        .await
        .unwrap();

        let row: SqliteRow = sqlx::query("SELECT * FROM users WHERE pubkey = ?")
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
        sqlx::query("DELETE FROM users WHERE pubkey = ?")
            .bind(test_pubkey.to_hex())
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO users (pubkey, metadata, created_at, updated_at) VALUES (?, ?, ?, ?)",
        )
        .bind(test_pubkey.to_hex())
        .bind(&test_metadata_json)
        .bind(0i64) // Valid timestamp for created_at
        .bind(invalid_timestamp)
        .execute(&pool)
        .await
        .unwrap();

        let row: SqliteRow = sqlx::query("SELECT * FROM users WHERE pubkey = ?")
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

    #[tokio::test]
    async fn test_save_user_success() {
        use crate::whitenoise::test_utils::create_mock_whitenoise;

        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create test user
        let test_pubkey = nostr_sdk::Keys::generate().public_key();
        let test_metadata = Metadata::new()
            .name("Test User")
            .display_name("Test Display Name")
            .about("Test about section");
        let test_created_at = chrono::Utc::now();
        let test_updated_at = chrono::Utc::now();

        let user = User {
            id: 1, // Will be overridden by database auto-increment
            pubkey: test_pubkey,
            metadata: test_metadata.clone(),
            created_at: test_created_at,
            updated_at: test_updated_at,
        };

        // Test save_user
        let result = whitenoise.save_user(&user).await;
        assert!(result.is_ok());

        // Test that we can load it back (this verifies it was saved correctly)
        let loaded_user = whitenoise.load_user(&test_pubkey).await;
        assert!(loaded_user.is_ok());

        let loaded = loaded_user.unwrap();
        assert_eq!(loaded.pubkey, test_pubkey);
        assert_eq!(loaded.metadata.name, test_metadata.name);
        assert_eq!(loaded.metadata.display_name, test_metadata.display_name);
        assert_eq!(loaded.metadata.about, test_metadata.about);
        assert_eq!(
            loaded.created_at.timestamp_millis(),
            test_created_at.timestamp_millis()
        );
        assert_eq!(
            loaded.updated_at.timestamp_millis(),
            test_updated_at.timestamp_millis()
        );
    }

    #[tokio::test]
    async fn test_save_user_with_minimal_metadata() {
        use crate::whitenoise::test_utils::create_mock_whitenoise;

        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        let test_pubkey = nostr_sdk::Keys::generate().public_key();
        let test_metadata = Metadata::new(); // Minimal metadata
        let test_created_at = chrono::Utc::now();
        let test_updated_at = chrono::Utc::now();

        let user = User {
            id: 1,
            pubkey: test_pubkey,
            metadata: test_metadata.clone(),
            created_at: test_created_at,
            updated_at: test_updated_at,
        };

        let result = whitenoise.save_user(&user).await;
        assert!(result.is_ok());

        // Verify it was saved correctly by loading it back
        let loaded_user = whitenoise.load_user(&test_pubkey).await;
        assert!(loaded_user.is_ok());

        let loaded = loaded_user.unwrap();
        assert_eq!(loaded.metadata.name, None);
        assert_eq!(
            loaded.created_at.timestamp_millis(),
            test_created_at.timestamp_millis()
        );
        assert_eq!(
            loaded.updated_at.timestamp_millis(),
            test_updated_at.timestamp_millis()
        );
    }

    #[tokio::test]
    async fn test_load_user_not_found() {
        use crate::whitenoise::test_utils::create_mock_whitenoise;

        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Try to load a non-existent user
        let non_existent_pubkey = nostr_sdk::Keys::generate().public_key();
        let result = whitenoise.load_user(&non_existent_pubkey).await;

        assert!(result.is_err());
        if let Err(WhitenoiseError::UserNotFound) = result {
            // Expected error
        } else {
            panic!("Expected UserNotFound error");
        }
    }

    #[tokio::test]
    async fn test_save_and_load_user_roundtrip() {
        use crate::whitenoise::test_utils::create_mock_whitenoise;

        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create test user with comprehensive metadata
        let test_pubkey = nostr_sdk::Keys::generate().public_key();
        let test_metadata = Metadata::new()
            .name("Complete Test User")
            .display_name("Complete Display Name")
            .about("Complete test about section")
            .nip05("test@example.com")
            .lud06("lnurl1dp68gurn8ghj7urp0v4kxvern9eehqurfdcsk6arpdd5kuemmduhxcmmrdaehgu3wd3skuep0dejhctnwda3kxvd09eszuekd0v8rqnrpwcxk7trj0ae8gmmwv9unx2txvg6xqmwpwejkcmmfd9c");
        let test_created_at = chrono::Utc::now();
        let test_updated_at = chrono::Utc::now();

        let original_user = User {
            id: 1,
            pubkey: test_pubkey,
            metadata: test_metadata.clone(),
            created_at: test_created_at,
            updated_at: test_updated_at,
        };

        // Save the user
        let save_result = whitenoise.save_user(&original_user).await;
        assert!(save_result.is_ok());

        // Load the user back
        let loaded_user = whitenoise.load_user(&test_pubkey).await;
        assert!(loaded_user.is_ok());

        let user = loaded_user.unwrap();

        // Verify all fields match (except id which is set by database)
        assert_eq!(user.pubkey, original_user.pubkey);
        assert_eq!(user.metadata.name, original_user.metadata.name);
        assert_eq!(
            user.metadata.display_name,
            original_user.metadata.display_name
        );
        assert_eq!(user.metadata.about, original_user.metadata.about);
        assert_eq!(user.metadata.picture, original_user.metadata.picture);
        assert_eq!(user.metadata.banner, original_user.metadata.banner);
        assert_eq!(user.metadata.nip05, original_user.metadata.nip05);
        assert_eq!(user.metadata.lud06, original_user.metadata.lud06);
        assert_eq!(
            user.created_at.timestamp_millis(),
            original_user.created_at.timestamp_millis()
        );
        assert_eq!(
            user.updated_at.timestamp_millis(),
            original_user.updated_at.timestamp_millis()
        );
    }

    #[tokio::test]
    async fn test_save_user_with_complex_metadata() {
        use crate::whitenoise::test_utils::create_mock_whitenoise;

        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Test with various metadata combinations
        let test_cases = [
            (
                "User with all fields",
                Metadata::new()
                    .name("Full User")
                    .display_name("Full Display")
                    .picture(nostr::types::url::Url::parse("https://example.com/avatar.jpg").unwrap())
                    .banner(nostr::types::url::Url::parse("https://example.com/banner.jpg").unwrap())
                    .about("Full about")
                    .nip05("full@example.com")
                    .lud06("lnurl1dp68gurn8ghj7urp0v4kxvern9eehqurfdcsk6arpdd5kuemmduhxcmmrdaehgu3wd3skuep0dejhctnwda3kxvd09eszuekd0v8rqnrpwcxk7trj0ae8gmmwv9unx2txvg6xqmwpwejkcmmfd9c"),
            ),
            (
                "User with only name",
                Metadata::new().name("Name Only"),
            ),
            (
                "User with name and about",
                Metadata::new().name("Named User").about("Has about section"),
            ),
            (
                "User with empty metadata",
                Metadata::new(),
            ),
        ];

        let test_timestamp = chrono::Utc::now();

        for (description, metadata) in test_cases {
            let test_pubkey = nostr_sdk::Keys::generate().public_key();

            let user = User {
                id: 1,
                pubkey: test_pubkey,
                metadata: metadata.clone(),
                created_at: test_timestamp,
                updated_at: test_timestamp,
            };

            // Save the user
            let save_result = whitenoise.save_user(&user).await;
            assert!(save_result.is_ok(), "Failed to save {}", description);

            // Load the user back
            let loaded_user = whitenoise.load_user(&test_pubkey).await;
            assert!(loaded_user.is_ok(), "Failed to load {}", description);

            let loaded = loaded_user.unwrap();
            assert_eq!(
                loaded.metadata.name, metadata.name,
                "{}: name mismatch",
                description
            );
            assert_eq!(
                loaded.metadata.display_name, metadata.display_name,
                "{}: display_name mismatch",
                description
            );
            assert_eq!(
                loaded.metadata.about, metadata.about,
                "{}: about mismatch",
                description
            );
            assert_eq!(
                loaded.metadata.picture, metadata.picture,
                "{}: picture mismatch",
                description
            );
            assert_eq!(
                loaded.metadata.banner, metadata.banner,
                "{}: banner mismatch",
                description
            );
            assert_eq!(
                loaded.metadata.nip05, metadata.nip05,
                "{}: nip05 mismatch",
                description
            );
            assert_eq!(
                loaded.metadata.lud06, metadata.lud06,
                "{}: lud06 mismatch",
                description
            );
        }
    }

    #[tokio::test]
    async fn test_load_user_relays_success() {
        use crate::whitenoise::test_utils::create_mock_whitenoise;

        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create test user
        let test_pubkey = nostr_sdk::Keys::generate().public_key();
        let test_metadata = Metadata::new().name("Test User");
        let test_timestamp = chrono::Utc::now();

        let user = User {
            id: 1,
            pubkey: test_pubkey,
            metadata: test_metadata.clone(),
            created_at: test_timestamp,
            updated_at: test_timestamp,
        };

        // Save the user first
        let save_result = whitenoise.save_user(&user).await;
        assert!(save_result.is_ok());

        // Load the user to get the actual database ID
        let loaded_user = whitenoise.load_user(&test_pubkey).await.unwrap();

        // Create test relays
        let relay1_url = nostr_sdk::RelayUrl::parse("wss://relay1.example.com").unwrap();
        let relay2_url = nostr_sdk::RelayUrl::parse("wss://relay2.example.com").unwrap();
        let relay3_url = nostr_sdk::RelayUrl::parse("wss://relay3.example.com").unwrap();

        let relay1 = Relay {
            id: 1,
            url: relay1_url.clone(),
            created_at: test_timestamp,
            updated_at: test_timestamp,
        };
        let relay2 = Relay {
            id: 2,
            url: relay2_url.clone(),
            created_at: test_timestamp,
            updated_at: test_timestamp,
        };
        let relay3 = Relay {
            id: 3,
            url: relay3_url.clone(),
            created_at: test_timestamp,
            updated_at: test_timestamp,
        };

        // Save relays to database
        whitenoise.save_relay(&relay1).await.unwrap();
        whitenoise.save_relay(&relay2).await.unwrap();
        whitenoise.save_relay(&relay3).await.unwrap();

        // Insert into user_relays table
        sqlx::query(
            "INSERT INTO user_relays (user_id, relay_id, relay_type, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(loaded_user.id)
        .bind(1) // relay1
        .bind("nostr")
        .bind(test_timestamp.timestamp_millis())
        .bind(test_timestamp.timestamp_millis())
        .execute(&whitenoise.database.pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO user_relays (user_id, relay_id, relay_type, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(loaded_user.id)
        .bind(2) // relay2
        .bind("nostr")
        .bind(test_timestamp.timestamp_millis())
        .bind(test_timestamp.timestamp_millis())
        .execute(&whitenoise.database.pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO user_relays (user_id, relay_id, relay_type, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(loaded_user.id)
        .bind(3) // relay3
        .bind("inbox")
        .bind(test_timestamp.timestamp_millis())
        .bind(test_timestamp.timestamp_millis())
        .execute(&whitenoise.database.pool)
        .await
        .unwrap();

        // Test loading nostr relays
        let nostr_relays = whitenoise
            .load_user_relays(&loaded_user, RelayType::Nostr)
            .await
            .unwrap();

        assert_eq!(nostr_relays.len(), 2);
        let relay_urls: Vec<_> = nostr_relays.iter().map(|r| &r.url).collect();
        assert!(relay_urls.contains(&&relay1_url));
        assert!(relay_urls.contains(&&relay2_url));
        assert!(!relay_urls.contains(&&relay3_url));

        // Test loading inbox relays
        let inbox_relays = whitenoise
            .load_user_relays(&loaded_user, RelayType::Inbox)
            .await
            .unwrap();

        assert_eq!(inbox_relays.len(), 1);
        assert_eq!(inbox_relays[0].url, relay3_url);

        // Test loading key package relays (should be empty)
        let key_package_relays = whitenoise
            .load_user_relays(&loaded_user, RelayType::KeyPackage)
            .await
            .unwrap();

        assert_eq!(key_package_relays.len(), 0);
    }

    #[tokio::test]
    async fn test_load_user_relays_empty_result() {
        use crate::whitenoise::test_utils::create_mock_whitenoise;

        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create test user
        let test_pubkey = nostr_sdk::Keys::generate().public_key();
        let test_metadata = Metadata::new().name("Test User");
        let test_timestamp = chrono::Utc::now();

        let user = User {
            id: 1,
            pubkey: test_pubkey,
            metadata: test_metadata.clone(),
            created_at: test_timestamp,
            updated_at: test_timestamp,
        };

        // Save the user first
        whitenoise.save_user(&user).await.unwrap();
        let loaded_user = whitenoise.load_user(&test_pubkey).await.unwrap();

        // Test loading relays when none exist
        let result = whitenoise
            .load_user_relays(&loaded_user, RelayType::Nostr)
            .await
            .unwrap();

        assert_eq!(result.len(), 0);
    }

    #[tokio::test]
    async fn test_load_user_relays_multiple_relay_types() {
        use crate::whitenoise::test_utils::create_mock_whitenoise;

        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create test user
        let test_pubkey = nostr_sdk::Keys::generate().public_key();
        let test_metadata = Metadata::new().name("Test User");
        let test_timestamp = chrono::Utc::now();

        let user = User {
            id: 1,
            pubkey: test_pubkey,
            metadata: test_metadata.clone(),
            created_at: test_timestamp,
            updated_at: test_timestamp,
        };

        whitenoise.save_user(&user).await.unwrap();
        let loaded_user = whitenoise.load_user(&test_pubkey).await.unwrap();

        // Create and save a test relay
        let relay_url = nostr_sdk::RelayUrl::parse("wss://multi.example.com").unwrap();
        let relay = Relay {
            id: 1,
            url: relay_url.clone(),
            created_at: test_timestamp,
            updated_at: test_timestamp,
        };
        whitenoise.save_relay(&relay).await.unwrap();

        // Add the same relay for different types
        for relay_type in ["nostr", "inbox", "key_package"] {
            sqlx::query(
                "INSERT INTO user_relays (user_id, relay_id, relay_type, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(loaded_user.id)
            .bind(1)
            .bind(relay_type)
            .bind(test_timestamp.timestamp_millis())
            .bind(test_timestamp.timestamp_millis())
            .execute(&whitenoise.database.pool)
            .await
            .unwrap();
        }

        // Test each relay type returns the same relay
        for relay_type in [RelayType::Nostr, RelayType::Inbox, RelayType::KeyPackage] {
            let relays = whitenoise
                .load_user_relays(&loaded_user, relay_type)
                .await
                .unwrap();

            assert_eq!(relays.len(), 1);
            assert_eq!(relays[0].url, relay_url);
        }
    }

    #[tokio::test]
    async fn test_load_user_relays_different_users() {
        use crate::whitenoise::test_utils::create_mock_whitenoise;

        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create two test users
        let user1_pubkey = nostr_sdk::Keys::generate().public_key();
        let user2_pubkey = nostr_sdk::Keys::generate().public_key();
        let test_timestamp = chrono::Utc::now();

        let user1 = User {
            id: 1,
            pubkey: user1_pubkey,
            metadata: Metadata::new().name("User 1"),
            created_at: test_timestamp,
            updated_at: test_timestamp,
        };

        let user2 = User {
            id: 2,
            pubkey: user2_pubkey,
            metadata: Metadata::new().name("User 2"),
            created_at: test_timestamp,
            updated_at: test_timestamp,
        };

        whitenoise.save_user(&user1).await.unwrap();
        whitenoise.save_user(&user2).await.unwrap();

        let loaded_user1 = whitenoise.load_user(&user1_pubkey).await.unwrap();
        let loaded_user2 = whitenoise.load_user(&user2_pubkey).await.unwrap();

        // Create test relays
        let relay1_url = nostr_sdk::RelayUrl::parse("wss://user1.example.com").unwrap();
        let relay2_url = nostr_sdk::RelayUrl::parse("wss://user2.example.com").unwrap();

        let relay1 = Relay {
            id: 1,
            url: relay1_url.clone(),
            created_at: test_timestamp,
            updated_at: test_timestamp,
        };
        let relay2 = Relay {
            id: 2,
            url: relay2_url.clone(),
            created_at: test_timestamp,
            updated_at: test_timestamp,
        };

        whitenoise.save_relay(&relay1).await.unwrap();
        whitenoise.save_relay(&relay2).await.unwrap();

        // Associate relay1 with user1 and relay2 with user2
        sqlx::query(
            "INSERT INTO user_relays (user_id, relay_id, relay_type, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(loaded_user1.id)
        .bind(1)
        .bind("nostr")
        .bind(test_timestamp.timestamp_millis())
        .bind(test_timestamp.timestamp_millis())
        .execute(&whitenoise.database.pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO user_relays (user_id, relay_id, relay_type, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(loaded_user2.id)
        .bind(2)
        .bind("nostr")
        .bind(test_timestamp.timestamp_millis())
        .bind(test_timestamp.timestamp_millis())
        .execute(&whitenoise.database.pool)
        .await
        .unwrap();

        // Test that each user gets only their own relays
        let user1_relays = whitenoise
            .load_user_relays(&loaded_user1, RelayType::Nostr)
            .await
            .unwrap();
        let user2_relays = whitenoise
            .load_user_relays(&loaded_user2, RelayType::Nostr)
            .await
            .unwrap();

        assert_eq!(user1_relays.len(), 1);
        assert_eq!(user1_relays[0].url, relay1_url);

        assert_eq!(user2_relays.len(), 1);
        assert_eq!(user2_relays[0].url, relay2_url);
    }

    #[tokio::test]
    async fn test_load_user_relays_relay_properties() {
        use crate::whitenoise::test_utils::create_mock_whitenoise;

        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create test user
        let test_pubkey = nostr_sdk::Keys::generate().public_key();
        let test_metadata = Metadata::new().name("Test User");
        let test_timestamp = chrono::Utc::now();

        let user = User {
            id: 1,
            pubkey: test_pubkey,
            metadata: test_metadata.clone(),
            created_at: test_timestamp,
            updated_at: test_timestamp,
        };

        whitenoise.save_user(&user).await.unwrap();
        let loaded_user = whitenoise.load_user(&test_pubkey).await.unwrap();

        // Create test relay with specific timestamps
        let relay_url = nostr_sdk::RelayUrl::parse("wss://properties.example.com").unwrap();
        let relay_created_at = chrono::Utc::now() - chrono::Duration::hours(1);
        let relay_updated_at = chrono::Utc::now() - chrono::Duration::minutes(30);

        let relay = Relay {
            id: 1,
            url: relay_url.clone(),
            created_at: relay_created_at,
            updated_at: relay_updated_at,
        };

        whitenoise.save_relay(&relay).await.unwrap();

        // Associate with user
        sqlx::query(
            "INSERT INTO user_relays (user_id, relay_id, relay_type, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(loaded_user.id)
        .bind(1)
        .bind("nostr")
        .bind(test_timestamp.timestamp_millis())
        .bind(test_timestamp.timestamp_millis())
        .execute(&whitenoise.database.pool)
        .await
        .unwrap();

        // Load relays and verify all properties
        let relays = whitenoise
            .load_user_relays(&loaded_user, RelayType::Nostr)
            .await
            .unwrap();

        assert_eq!(relays.len(), 1);
        let loaded_relay = &relays[0];

        assert_eq!(loaded_relay.url, relay_url);
        assert_eq!(
            loaded_relay.created_at.timestamp_millis(),
            relay_created_at.timestamp_millis()
        );
        assert_eq!(
            loaded_relay.updated_at.timestamp_millis(),
            relay_updated_at.timestamp_millis()
        );
        // ID should be set by database
        assert!(loaded_relay.id > 0);
    }
}
