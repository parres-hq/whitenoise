use super::DatabaseError;
use crate::whitenoise::accounts::{Account, AccountNew, AccountSettings};
use crate::{Whitenoise, WhitenoiseError};
use chrono::{DateTime, Utc};
use nostr_sdk::PublicKey;

#[allow(dead_code)]
struct AccountNewRow {
    // id is the primary key
    id: i64,
    // pubkey is the hex encoded nostr public key
    pubkey: PublicKey,
    // user_id is the foreign key to the users table
    user_id: i64,
    // settings is the JSONB column that stores the account settings
    settings: AccountSettings,
    // last_synced_at is the timestamp of the last sync (using the background fetch)
    last_synced_at: Option<DateTime<Utc>>,
    // created_at is the timestamp of the account creation
    created_at: DateTime<Utc>,
    // updated_at is the timestamp of the last update
    updated_at: DateTime<Utc>,
}

impl<'r, R> sqlx::FromRow<'r, R> for AccountNewRow
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
        let user_id: i64 = row.try_get("user_id")?;
        let settings_json: String = row.try_get("settings")?;
        let last_synced_at_i64: Option<i64> = row.try_get("last_synced_at")?;
        let created_at_i64: i64 = row.try_get("created_at")?;
        let updated_at_i64: i64 = row.try_get("updated_at")?;

        // Parse pubkey from hex string
        let pubkey = PublicKey::parse(&pubkey_str).map_err(|e| sqlx::Error::ColumnDecode {
            index: "pubkey".to_string(),
            source: Box::new(e),
        })?;

        // Parse settings from JSON
        let settings: AccountSettings =
            serde_json::from_str(&settings_json).map_err(|e| sqlx::Error::ColumnDecode {
                index: "settings".to_string(),
                source: Box::new(e),
            })?;

        // Convert timestamps from i64 to DateTime
        let last_synced_at = match last_synced_at_i64 {
            Some(timestamp) => Some(
                DateTime::from_timestamp_millis(timestamp)
                    .ok_or_else(|| DatabaseError::InvalidTimestamp { timestamp })
                    .map_err(|e| sqlx::Error::ColumnDecode {
                        index: "last_synced_at".to_string(),
                        source: Box::new(e),
                    })?,
            ),
            None => None,
        };

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

        Ok(AccountNewRow {
            id,
            pubkey,
            user_id,
            settings,
            last_synced_at,
            created_at,
            updated_at,
        })
    }
}

impl Whitenoise {
    pub async fn load_account_new(
        &self,
        pubkey: &PublicKey,
    ) -> Result<AccountNew, WhitenoiseError> {
        let account_row =
            sqlx::query_as::<_, AccountNewRow>("SELECT * FROM accounts_new WHERE pubkey = ?")
                .bind(pubkey.to_hex().as_str())
                .fetch_one(&self.database.pool)
                .await
                .map_err(|_| WhitenoiseError::AccountNotFound)?;

        Ok(AccountNew {
            id: account_row.id,
            user_id: account_row.user_id,
            pubkey: account_row.pubkey,
            settings: account_row.settings,
            last_synced_at: account_row.last_synced_at,
            created_at: account_row.created_at,
            updated_at: account_row.updated_at,
            nostr_mls: Account::create_nostr_mls(account_row.pubkey, &self.config.data_dir)?,
        })
    }

    pub async fn save_account_new(&self, account: &AccountNew) -> Result<(), DatabaseError> {
        sqlx::query("INSERT INTO accounts_new (pubkey, user_id, settings, last_synced_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(account.pubkey.to_hex().as_str())
            .bind(account.user_id)
            .bind(serde_json::to_string(&account.settings).unwrap())
            .bind(account.last_synced_at.map(|ts| ts.timestamp_millis()))
            .bind(account.created_at.timestamp_millis())
            .bind(account.updated_at.timestamp_millis())
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

    // Helper function to create a test database
    async fn setup_test_db() -> SqlitePool {
        let pool = SqlitePool::connect(":memory:").await.unwrap();

        // Create the accounts_new table with the schema from migration
        sqlx::query(
            "CREATE TABLE accounts_new (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                pubkey TEXT NOT NULL,
                user_id INTEGER NOT NULL,
                settings JSONB NOT NULL,
                last_synced_at DATETIME,
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
    async fn test_account_new_row_from_row_valid_data() {
        let pool = setup_test_db().await;

        // Create valid test data
        let test_pubkey = nostr_sdk::Keys::generate().public_key();
        let test_user_id = 42i64;
        let test_settings = AccountSettings {
            dark_theme: true,
            dev_mode: false,
            lockdown_mode: true,
        };
        let test_settings_json = serde_json::to_string(&test_settings).unwrap();
        let test_last_synced = chrono::Utc::now().timestamp_millis();
        let test_created_at = chrono::Utc::now().timestamp_millis();
        let test_updated_at = chrono::Utc::now().timestamp_millis();

        // Insert test data
        sqlx::query(
            "INSERT INTO accounts_new (pubkey, user_id, settings, last_synced_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(test_pubkey.to_hex())
        .bind(test_user_id)
        .bind(test_settings_json)
        .bind(test_last_synced)
        .bind(test_created_at)
        .bind(test_updated_at)
        .execute(&pool)
        .await
        .unwrap();

        // Test from_row implementation
        let row: SqliteRow = sqlx::query("SELECT * FROM accounts_new WHERE pubkey = ?")
            .bind(test_pubkey.to_hex())
            .fetch_one(&pool)
            .await
            .unwrap();

        let account_row = AccountNewRow::from_row(&row).unwrap();

        assert_eq!(account_row.pubkey, test_pubkey);
        assert_eq!(account_row.user_id, test_user_id);
        assert_eq!(account_row.settings, test_settings);
        assert!(account_row.last_synced_at.is_some());
        assert_eq!(
            account_row.last_synced_at.unwrap().timestamp_millis(),
            test_last_synced
        );
        assert_eq!(account_row.created_at.timestamp_millis(), test_created_at);
        assert_eq!(account_row.updated_at.timestamp_millis(), test_updated_at);
    }

    #[tokio::test]
    async fn test_account_new_row_from_row_null_last_synced() {
        let pool = setup_test_db().await;

        let test_pubkey = nostr_sdk::Keys::generate().public_key();
        let test_user_id = 123i64;
        let test_settings = AccountSettings::default(); // Use default settings
        let test_settings_json = serde_json::to_string(&test_settings).unwrap();
        let test_timestamp = chrono::Utc::now().timestamp_millis();

        // Insert test data with NULL last_synced_at
        sqlx::query(
            "INSERT INTO accounts_new (pubkey, user_id, settings, last_synced_at, created_at, updated_at) VALUES (?, ?, ?, NULL, ?, ?)",
        )
        .bind(test_pubkey.to_hex())
        .bind(test_user_id)
        .bind(test_settings_json)
        .bind(test_timestamp)
        .bind(test_timestamp)
        .execute(&pool)
        .await
        .unwrap();

        let row: SqliteRow = sqlx::query("SELECT * FROM accounts_new WHERE pubkey = ?")
            .bind(test_pubkey.to_hex())
            .fetch_one(&pool)
            .await
            .unwrap();

        let account_row = AccountNewRow::from_row(&row).unwrap();

        assert_eq!(account_row.pubkey, test_pubkey);
        assert_eq!(account_row.user_id, test_user_id);
        assert_eq!(account_row.settings, test_settings);
        assert!(account_row.last_synced_at.is_none());
        assert_eq!(account_row.created_at.timestamp_millis(), test_timestamp);
        assert_eq!(account_row.updated_at.timestamp_millis(), test_timestamp);
    }

    #[tokio::test]
    async fn test_account_new_row_from_row_various_settings() {
        let pool = setup_test_db().await;

        let test_cases = [
            AccountSettings {
                dark_theme: true,
                dev_mode: true,
                lockdown_mode: true,
            },
            AccountSettings {
                dark_theme: false,
                dev_mode: false,
                lockdown_mode: false,
            },
            AccountSettings::default(),
        ];

        let test_timestamp = chrono::Utc::now().timestamp_millis();

        for (i, settings) in test_cases.iter().enumerate() {
            let test_pubkey = nostr_sdk::Keys::generate().public_key();
            let test_user_id = (i + 1) as i64;
            let settings_json = serde_json::to_string(settings).unwrap();

            // Insert test data
            sqlx::query(
                "INSERT INTO accounts_new (pubkey, user_id, settings, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(test_pubkey.to_hex())
            .bind(test_user_id)
            .bind(settings_json)
            .bind(test_timestamp)
            .bind(test_timestamp)
            .execute(&pool)
            .await
            .unwrap();

            // Test from_row implementation
            let row: SqliteRow = sqlx::query("SELECT * FROM accounts_new WHERE pubkey = ?")
                .bind(test_pubkey.to_hex())
                .fetch_one(&pool)
                .await
                .unwrap();

            let account_row = AccountNewRow::from_row(&row).unwrap();
            assert_eq!(account_row.settings, *settings);

            // Clean up
            sqlx::query("DELETE FROM accounts_new WHERE pubkey = ?")
                .bind(test_pubkey.to_hex())
                .execute(&pool)
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn test_account_new_row_from_row_invalid_pubkey() {
        let pool = setup_test_db().await;

        let invalid_pubkeys = [
            "not_a_pubkey",
            "too_short",
            "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz", // Invalid hex
            "",
        ];

        let test_settings = AccountSettings::default();
        let test_settings_json = serde_json::to_string(&test_settings).unwrap();
        let test_timestamp = chrono::Utc::now().timestamp_millis();

        for (i, invalid_pubkey) in invalid_pubkeys.iter().enumerate() {
            let test_user_id = (i + 1) as i64;

            // Insert invalid pubkey
            sqlx::query(
                "INSERT INTO accounts_new (pubkey, user_id, settings, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(invalid_pubkey)
            .bind(test_user_id)
            .bind(&test_settings_json)
            .bind(test_timestamp)
            .bind(test_timestamp)
            .execute(&pool)
            .await
            .unwrap();

            // Test from_row implementation should fail
            let row: SqliteRow = sqlx::query("SELECT * FROM accounts_new WHERE pubkey = ?")
                .bind(invalid_pubkey)
                .fetch_one(&pool)
                .await
                .unwrap();

            let result = AccountNewRow::from_row(&row);
            assert!(result.is_err());

            if let Err(sqlx::Error::ColumnDecode { index, .. }) = result {
                assert_eq!(index, "pubkey");
            } else {
                panic!("Expected ColumnDecode error for pubkey");
            }

            // Clean up
            sqlx::query("DELETE FROM accounts_new WHERE pubkey = ?")
                .bind(invalid_pubkey)
                .execute(&pool)
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn test_account_new_row_from_row_invalid_settings_json() {
        let pool = setup_test_db().await;

        let test_pubkey = nostr_sdk::Keys::generate().public_key();
        let test_user_id = 999i64;
        let invalid_json_values = [
            "not_json",
            "{incomplete_json",
            "[]", // Array instead of object
            r#"{"missing_required_fields": true}"#,
            r#"{"dark_theme": "not_a_boolean"}"#, // Wrong type
        ];
        let test_timestamp = chrono::Utc::now().timestamp_millis();

        for invalid_json in invalid_json_values {
            // Insert invalid settings JSON
            sqlx::query(
                "INSERT INTO accounts_new (pubkey, user_id, settings, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(test_pubkey.to_hex())
            .bind(test_user_id)
            .bind(invalid_json)
            .bind(test_timestamp)
            .bind(test_timestamp)
            .execute(&pool)
            .await
            .unwrap();

            // Test from_row implementation should fail
            let row: SqliteRow = sqlx::query("SELECT * FROM accounts_new WHERE pubkey = ?")
                .bind(test_pubkey.to_hex())
                .fetch_one(&pool)
                .await
                .unwrap();

            let result = AccountNewRow::from_row(&row);
            assert!(result.is_err());

            if let Err(sqlx::Error::ColumnDecode { index, .. }) = result {
                assert_eq!(index, "settings");
            } else {
                panic!("Expected ColumnDecode error for settings");
            }

            // Clean up
            sqlx::query("DELETE FROM accounts_new WHERE pubkey = ?")
                .bind(test_pubkey.to_hex())
                .execute(&pool)
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn test_account_new_row_from_row_invalid_timestamps() {
        let pool = setup_test_db().await;

        let test_pubkey = nostr_sdk::Keys::generate().public_key();
        let test_user_id = 777i64;
        let test_settings = AccountSettings::default();
        let test_settings_json = serde_json::to_string(&test_settings).unwrap();
        let valid_timestamp = chrono::Utc::now().timestamp_millis();
        let invalid_timestamp = i64::MAX; // This will be too large for DateTime conversion

        // Test invalid last_synced_at timestamp
        sqlx::query(
            "INSERT INTO accounts_new (pubkey, user_id, settings, last_synced_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(test_pubkey.to_hex())
        .bind(test_user_id)
        .bind(&test_settings_json)
        .bind(invalid_timestamp)
        .bind(valid_timestamp)
        .bind(valid_timestamp)
        .execute(&pool)
        .await
        .unwrap();

        let row: SqliteRow = sqlx::query("SELECT * FROM accounts_new WHERE pubkey = ?")
            .bind(test_pubkey.to_hex())
            .fetch_one(&pool)
            .await
            .unwrap();

        let result = AccountNewRow::from_row(&row);
        assert!(result.is_err());

        if let Err(sqlx::Error::ColumnDecode { index, .. }) = result {
            assert_eq!(index, "last_synced_at");
        } else {
            panic!("Expected ColumnDecode error for last_synced_at timestamp");
        }

        // Clean up and test invalid created_at timestamp
        sqlx::query("DELETE FROM accounts_new WHERE pubkey = ?")
            .bind(test_pubkey.to_hex())
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO accounts_new (pubkey, user_id, settings, last_synced_at, created_at, updated_at) VALUES (?, ?, ?, NULL, ?, ?)",
        )
        .bind(test_pubkey.to_hex())
        .bind(test_user_id)
        .bind(&test_settings_json)
        .bind(invalid_timestamp)
        .bind(valid_timestamp)
        .execute(&pool)
        .await
        .unwrap();

        let row: SqliteRow = sqlx::query("SELECT * FROM accounts_new WHERE pubkey = ?")
            .bind(test_pubkey.to_hex())
            .fetch_one(&pool)
            .await
            .unwrap();

        let result = AccountNewRow::from_row(&row);
        assert!(result.is_err());

        if let Err(sqlx::Error::ColumnDecode { index, .. }) = result {
            assert_eq!(index, "created_at");
        } else {
            panic!("Expected ColumnDecode error for created_at timestamp");
        }

        // Clean up and test invalid updated_at timestamp
        sqlx::query("DELETE FROM accounts_new WHERE pubkey = ?")
            .bind(test_pubkey.to_hex())
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO accounts_new (pubkey, user_id, settings, last_synced_at, created_at, updated_at) VALUES (?, ?, ?, NULL, ?, ?)",
        )
        .bind(test_pubkey.to_hex())
        .bind(test_user_id)
        .bind(&test_settings_json)
        .bind(valid_timestamp)
        .bind(invalid_timestamp)
        .execute(&pool)
        .await
        .unwrap();

        let row: SqliteRow = sqlx::query("SELECT * FROM accounts_new WHERE pubkey = ?")
            .bind(test_pubkey.to_hex())
            .fetch_one(&pool)
            .await
            .unwrap();

        let result = AccountNewRow::from_row(&row);
        assert!(result.is_err());

        if let Err(sqlx::Error::ColumnDecode { index, .. }) = result {
            assert_eq!(index, "updated_at");
        } else {
            panic!("Expected ColumnDecode error for updated_at timestamp");
        }
    }

    #[tokio::test]
    async fn test_save_account_new_success() {
        use crate::whitenoise::test_utils::create_mock_whitenoise;

        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create test account
        let test_pubkey = nostr_sdk::Keys::generate().public_key();
        let test_user_id = 42i64;
        let test_settings = AccountSettings {
            dark_theme: false,
            dev_mode: true,
            lockdown_mode: false,
        };
        let test_last_synced = Some(chrono::Utc::now());
        let test_created_at = chrono::Utc::now();
        let test_updated_at = chrono::Utc::now();

        let account = AccountNew {
            id: 1, // Will be overridden by database auto-increment
            pubkey: test_pubkey,
            user_id: test_user_id,
            settings: test_settings.clone(),
            last_synced_at: test_last_synced,
            created_at: test_created_at,
            updated_at: test_updated_at,
            nostr_mls: Account::create_nostr_mls(test_pubkey, &whitenoise.config.data_dir).unwrap(),
        };

        // Test save_account_new
        let result = whitenoise.save_account_new(&account).await;
        assert!(result.is_ok());

        // Test that we can load it back (this verifies it was saved correctly)
        let loaded_account = whitenoise.load_account_new(&test_pubkey).await;
        assert!(loaded_account.is_ok());

        let loaded = loaded_account.unwrap();
        assert_eq!(loaded.pubkey, test_pubkey);
        assert_eq!(loaded.user_id, test_user_id);
        assert_eq!(loaded.settings, test_settings);
        assert!(loaded.last_synced_at.is_some());
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
    async fn test_save_account_new_with_null_last_synced() {
        use crate::whitenoise::test_utils::create_mock_whitenoise;

        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        let test_pubkey = nostr_sdk::Keys::generate().public_key();
        let test_user_id = 123i64;
        let test_settings = AccountSettings::default();
        let test_created_at = chrono::Utc::now();
        let test_updated_at = chrono::Utc::now();

        let account = AccountNew {
            id: 1,
            pubkey: test_pubkey,
            user_id: test_user_id,
            settings: test_settings.clone(),
            last_synced_at: None, // Test with None
            created_at: test_created_at,
            updated_at: test_updated_at,
            nostr_mls: Account::create_nostr_mls(test_pubkey, &whitenoise.config.data_dir).unwrap(),
        };

        let result = whitenoise.save_account_new(&account).await;
        assert!(result.is_ok());

        // Verify it was saved correctly by loading it back
        let loaded_account = whitenoise.load_account_new(&test_pubkey).await;
        assert!(loaded_account.is_ok());

        let loaded = loaded_account.unwrap();
        assert!(loaded.last_synced_at.is_none());
        assert_eq!(loaded.settings, test_settings);
    }

    #[tokio::test]
    async fn test_load_account_new_not_found() {
        use crate::whitenoise::test_utils::create_mock_whitenoise;

        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Try to load a non-existent account
        let non_existent_pubkey = nostr_sdk::Keys::generate().public_key();
        let result = whitenoise.load_account_new(&non_existent_pubkey).await;

        assert!(result.is_err());
        if let Err(WhitenoiseError::AccountNotFound) = result {
            // Expected error
        } else {
            panic!("Expected AccountNotFound error");
        }
    }

    #[tokio::test]
    async fn test_save_and_load_account_new_roundtrip() {
        use crate::whitenoise::test_utils::create_mock_whitenoise;

        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create test account with all fields
        let test_pubkey = nostr_sdk::Keys::generate().public_key();
        let test_user_id = 555i64;
        let test_settings = AccountSettings {
            dark_theme: true,
            dev_mode: true,
            lockdown_mode: false,
        };
        let test_last_synced = Some(chrono::Utc::now());
        let test_created_at = chrono::Utc::now();
        let test_updated_at = chrono::Utc::now();

        let original_account = AccountNew {
            id: 1,
            pubkey: test_pubkey,
            user_id: test_user_id,
            settings: test_settings.clone(),
            last_synced_at: test_last_synced,
            created_at: test_created_at,
            updated_at: test_updated_at,
            nostr_mls: Account::create_nostr_mls(test_pubkey, &whitenoise.config.data_dir).unwrap(),
        };

        // Save the account
        let save_result = whitenoise.save_account_new(&original_account).await;
        assert!(save_result.is_ok());

        // Load the account back
        let loaded_account = whitenoise.load_account_new(&test_pubkey).await;
        assert!(loaded_account.is_ok());

        let account = loaded_account.unwrap();

        // Verify all fields match (except id which is set by database)
        assert_eq!(account.pubkey, original_account.pubkey);
        assert_eq!(account.user_id, original_account.user_id);
        assert_eq!(account.settings, original_account.settings);
        assert_eq!(
            account.last_synced_at.map(|ts| ts.timestamp_millis()),
            original_account
                .last_synced_at
                .map(|ts| ts.timestamp_millis())
        );
        assert_eq!(
            account.created_at.timestamp_millis(),
            original_account.created_at.timestamp_millis()
        );
        assert_eq!(
            account.updated_at.timestamp_millis(),
            original_account.updated_at.timestamp_millis()
        );
    }
}
