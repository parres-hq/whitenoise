use chrono::{DateTime, Utc};
use nostr_sdk::PublicKey;
use sqlx::Row;

use super::{utils::parse_timestamp, Database, DatabaseError};
use crate::{
    whitenoise::{accounts::Account, database::users::UserRow, users::User},
    WhitenoiseError,
};

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
struct AccountRow {
    // id is the primary key
    id: i64,
    // pubkey is the hex encoded nostr public key
    pubkey: PublicKey,
    // user_id is the foreign key to the users table
    user_id: i64,
    // last_synced_at is the timestamp of the last sync (using the background fetch)
    last_synced_at: Option<DateTime<Utc>>,
    // created_at is the timestamp of the account creation
    created_at: DateTime<Utc>,
    // updated_at is the timestamp of the last update
    updated_at: DateTime<Utc>,
}

impl<'r, R> sqlx::FromRow<'r, R> for AccountRow
where
    R: sqlx::Row,
    &'r str: sqlx::ColumnIndex<R>,
    String: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
    i64: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
{
    fn from_row(row: &'r R) -> std::result::Result<Self, sqlx::Error> {
        let id: i64 = row.try_get("id")?;
        let pubkey_str: String = row.try_get("pubkey")?;
        let user_id: i64 = row.try_get("user_id")?;

        // Parse pubkey from hex string
        let pubkey = PublicKey::parse(&pubkey_str).map_err(|e| sqlx::Error::ColumnDecode {
            index: "pubkey".to_string(),
            source: Box::new(e),
        })?;

        let last_synced_at = match row.try_get::<Option<i64>, _>("last_synced_at")? {
            Some(_) => Some(parse_timestamp(row, "last_synced_at")?),
            None => None,
        };

        let created_at = parse_timestamp(row, "created_at")?;
        let updated_at = parse_timestamp(row, "updated_at")?;

        Ok(AccountRow {
            id,
            pubkey,
            user_id,
            last_synced_at,
            created_at,
            updated_at,
        })
    }
}

impl AccountRow {
    /// Converts an AccountRow to an Account by creating the required NostrMls instance.
    pub(crate) fn into_account(self) -> Result<Account, WhitenoiseError> {
        Ok(Account {
            id: Some(self.id),
            pubkey: self.pubkey,
            user_id: self.user_id,
            last_synced_at: self.last_synced_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

impl Account {
    /// Loads all accounts from the database.
    ///
    /// # Arguments
    ///
    /// * `database` - A reference to the `Database` instance for database operations
    ///
    /// # Returns
    ///
    /// Returns a `Vec<Account>` containing all accounts in the database on success.
    ///
    /// # Errors
    ///
    /// Returns a [`WhitenoiseError`] if the database query fails.
    pub(crate) async fn all(database: &Database) -> Result<Vec<Account>, WhitenoiseError> {
        let account_rows = sqlx::query_as::<_, AccountRow>("SELECT * FROM accounts")
            .fetch_all(&database.pool)
            .await
            .map_err(DatabaseError::Sqlx)?;

        account_rows
            .into_iter()
            .map(|row| row.into_account())
            .collect::<Result<Vec<Account>, WhitenoiseError>>()
    }

    /// Gets the oldest account from the database.
    ///
    /// # Arguments
    ///
    /// * `database` - A reference to the `Database` instance for database operations
    ///
    /// # Returns
    ///
    /// Returns the oldest `Account` from the database on success. If no account exists, return None.
    ///
    /// # Errors
    ///
    /// Returns a [`WhitenoiseError`] if the database query fails.
    pub(crate) async fn first(database: &Database) -> Result<Option<Account>, WhitenoiseError> {
        let row_opt = sqlx::query_as::<_, AccountRow>(
            "SELECT * FROM accounts ORDER BY created_at ASC, id ASC LIMIT 1",
        )
        .fetch_optional(&database.pool)
        .await
        .map_err(DatabaseError::Sqlx)?;

        row_opt.map(|row| row.into_account()).transpose()
    }

    /// Finds an account by its public key.
    ///
    /// # Arguments
    ///
    /// * `pubkey` - A reference to the `PublicKey` to search for
    /// * `database` - A reference to the `Database` instance for database operations
    ///
    /// # Returns
    ///
    /// Returns the `Account` associated with the provided public key on success.
    ///
    /// # Errors
    ///
    /// Returns a [`WhitenoiseError::AccountNotFound`] if no account with the given public key exists.
    pub(crate) async fn find_by_pubkey(
        pubkey: &PublicKey,
        database: &Database,
    ) -> Result<Account, WhitenoiseError> {
        let account_row =
            sqlx::query_as::<_, AccountRow>("SELECT * FROM accounts WHERE pubkey = ?")
                .bind(pubkey.to_hex().as_str())
                .fetch_one(&database.pool)
                .await
                .map_err(|_| WhitenoiseError::AccountNotFound)?;

        Ok(Account {
            id: Some(account_row.id),
            user_id: account_row.user_id,
            pubkey: account_row.pubkey,
            last_synced_at: account_row.last_synced_at,
            created_at: account_row.created_at,
            updated_at: account_row.updated_at,
        })
    }

    /// Gets the user associated with this account.
    ///
    /// # Arguments
    ///
    /// * `database` - A reference to the `Database` instance for database operations
    ///
    /// # Returns
    ///
    /// Returns the `User` associated with this account on success.
    ///
    /// # Errors
    ///
    /// Returns a [`WhitenoiseError::AccountNotFound`] if the associated user is not found.
    pub(crate) async fn user(&self, database: &Database) -> Result<User, WhitenoiseError> {
        let user_row = sqlx::query_as::<_, UserRow>("SELECT * FROM users WHERE id = ?")
            .bind(self.user_id)
            .fetch_one(&database.pool)
            .await
            .map_err(|_| WhitenoiseError::AccountNotFound)?;
        Ok(user_row.into())
    }

    /// Gets all users that this account follows.
    ///
    /// # Arguments
    ///
    /// * `database` - A reference to the `Database` instance for database operations
    ///
    /// # Returns
    ///
    /// Returns a `Vec<User>` containing all users followed by this account.
    ///
    /// # Errors
    ///
    /// Returns a [`WhitenoiseError::AccountNotFound`] if the database query fails.
    pub(crate) async fn follows(&self, database: &Database) -> Result<Vec<User>, WhitenoiseError> {
        let user_rows = sqlx::query_as::<_, UserRow>(
            "SELECT u.id, u.pubkey, u.metadata, u.created_at, u.updated_at
             FROM account_follows af
             JOIN users u ON af.user_id = u.id
             WHERE af.account_id = ?",
        )
        .bind(self.id)
        .fetch_all(&database.pool)
        .await
        .map_err(DatabaseError::Sqlx)?;

        let users = user_rows
            .into_iter()
            .map(|row| User {
                id: Some(row.id),
                pubkey: row.pubkey,
                metadata: row.metadata,
                created_at: row.created_at,
                updated_at: row.updated_at,
            })
            .collect();

        Ok(users)
    }

    /// Checks if this account is following a specific user.
    ///
    /// # Arguments
    ///
    /// * `user` - A reference to the `User` to check
    /// * `database` - A reference to the `Database` instance for database operations
    ///
    /// # Returns
    ///
    /// Returns `true` if this account is following the user, `false` otherwise.
    ///
    /// # Errors
    ///
    /// Returns a [`WhitenoiseError`] if the database query fails.
    pub(crate) async fn is_following_user(
        &self,
        user: &User,
        database: &Database,
    ) -> Result<bool, WhitenoiseError> {
        let result = sqlx::query(
            "SELECT COUNT(*) FROM account_follows WHERE account_id = ? AND user_id = ?",
        )
        .bind(self.id)
        .bind(user.id)
        .fetch_one(&database.pool)
        .await
        .map_err(DatabaseError::Sqlx)?;
        Ok(result.get::<i64, _>(0) > 0)
    }

    /// Makes this account follow a specific user.
    ///
    /// # Arguments
    ///
    /// * `user` - A reference to the `User` to follow
    /// * `database` - A reference to the `Database` instance for database operations
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success.
    ///
    /// # Errors
    ///
    /// Returns a [`WhitenoiseError`] if the database operation fails.
    pub(crate) async fn follow_user(
        &self,
        user: &User,
        database: &Database,
    ) -> Result<(), WhitenoiseError> {
        sqlx::query("INSERT INTO account_follows (account_id, user_id, created_at, updated_at) VALUES (?, ?, ?, ?) ON CONFLICT(account_id, user_id) DO UPDATE SET updated_at = ?")
            .bind(self.id)
            .bind(user.id)
            .bind(self.created_at.timestamp_millis())
            .bind(self.updated_at.timestamp_millis())
            .bind(Utc::now().timestamp_millis())
            .execute(&database.pool)
            .await
            .map_err(DatabaseError::Sqlx)?;
        Ok(())
    }

    /// Makes this account unfollow a specific user.
    ///
    /// # Arguments
    ///
    /// * `user` - A reference to the `User` to unfollow
    /// * `database` - A reference to the `Database` instance for database operations
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success.
    ///
    /// # Errors
    ///
    /// Returns a [`WhitenoiseError`] if the database operation fails.
    pub(crate) async fn unfollow_user(
        &self,
        user: &User,
        database: &Database,
    ) -> Result<(), WhitenoiseError> {
        sqlx::query("DELETE FROM account_follows WHERE account_id = ? AND user_id = ?")
            .bind(self.id)
            .bind(user.id)
            .execute(&database.pool)
            .await
            .map_err(DatabaseError::Sqlx)?;
        Ok(())
    }

    /// Saves this account to the database.
    ///
    /// # Arguments
    ///
    /// * `database` - A reference to the `Database` instance for database operations
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success.
    ///
    /// # Errors
    ///
    /// Returns a [`WhitenoiseError`] if the database operation fails.
    pub(crate) async fn save(&self, database: &Database) -> Result<(), WhitenoiseError> {
        sqlx::query("INSERT INTO accounts (pubkey, user_id, last_synced_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?) ON CONFLICT(pubkey) DO UPDATE SET user_id = excluded.user_id, last_synced_at = excluded.last_synced_at, updated_at = ?")
            .bind(self.pubkey.to_hex().as_str())
            .bind(self.user_id)
            .bind(self.last_synced_at.map(|ts| ts.timestamp_millis()))
            .bind(self.created_at.timestamp_millis())
            .bind(self.updated_at.timestamp_millis())
            .bind(Utc::now().timestamp_millis())
            .execute(&database.pool)
            .await
            .map_err(DatabaseError::Sqlx)?;
        Ok(())
    }

    /// Deletes this account from the database.
    ///
    /// # Arguments
    ///
    /// * `database` - A reference to the `Database` instance for database operations
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success.
    ///
    /// # Errors
    ///
    /// Returns a [`WhitenoiseError::AccountNotFound`] if the account doesn't exist in the database.
    /// Returns other [`WhitenoiseError`] variants if the database operation fails.
    pub(crate) async fn delete(&self, database: &Database) -> Result<(), WhitenoiseError> {
        let result = sqlx::query("DELETE FROM accounts WHERE pubkey = ?")
            .bind(self.pubkey.to_hex())
            .execute(&database.pool)
            .await?;

        tracing::debug!(target: "whitenoise::delete_account", "Account removed from database for pubkey: {}", self.pubkey.to_hex());

        if result.rows_affected() < 1 {
            Err(WhitenoiseError::AccountNotFound)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::whitenoise::test_utils::create_mock_whitenoise;

    use super::*;
    use sqlx::sqlite::SqliteRow;
    use sqlx::{FromRow, SqlitePool};

    // Helper function to create a test database
    async fn setup_test_db() -> SqlitePool {
        let pool = SqlitePool::connect(":memory:").await.unwrap();

        // Create the accounts table with the schema from migration
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

        // Create users table
        sqlx::query(
            "CREATE TABLE users (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                pubkey TEXT NOT NULL UNIQUE,
                metadata TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        // Create account_follows table
        sqlx::query(
            "CREATE TABLE account_follows (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                account_id INTEGER NOT NULL,
                user_id INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                FOREIGN KEY (account_id) REFERENCES accounts (id),
                FOREIGN KEY (user_id) REFERENCES users (id)
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    #[tokio::test]
    async fn test_account_row_from_row_valid_data() {
        let pool = setup_test_db().await;

        // Create valid test data
        let test_pubkey = nostr_sdk::Keys::generate().public_key();
        let test_user_id = 42i64;
        let test_last_synced = chrono::Utc::now().timestamp_millis();
        let test_created_at = chrono::Utc::now().timestamp_millis();
        let test_updated_at = chrono::Utc::now().timestamp_millis();

        // Insert test data
        sqlx::query(
            "INSERT INTO accounts (pubkey, user_id, last_synced_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(test_pubkey.to_hex())
        .bind(test_user_id)
        .bind(test_last_synced)
        .bind(test_created_at)
        .bind(test_updated_at)
        .execute(&pool)
        .await
        .unwrap();

        // Test from_row implementation
        let row: SqliteRow = sqlx::query("SELECT * FROM accounts WHERE pubkey = ?")
            .bind(test_pubkey.to_hex())
            .fetch_one(&pool)
            .await
            .unwrap();

        let account_row = AccountRow::from_row(&row).unwrap();

        assert_eq!(account_row.pubkey, test_pubkey);
        assert_eq!(account_row.user_id, test_user_id);
        assert!(account_row.last_synced_at.is_some());
        assert_eq!(
            account_row.last_synced_at.unwrap().timestamp_millis(),
            test_last_synced
        );
        assert_eq!(account_row.created_at.timestamp_millis(), test_created_at);
        assert_eq!(account_row.updated_at.timestamp_millis(), test_updated_at);
    }

    #[tokio::test]
    async fn test_account_row_from_row_null_last_synced() {
        let pool = setup_test_db().await;

        let test_pubkey = nostr_sdk::Keys::generate().public_key();
        let test_user_id = 123i64;
        let test_timestamp = chrono::Utc::now().timestamp_millis();

        // Insert test data with NULL last_synced_at
        sqlx::query(
            "INSERT INTO accounts (pubkey, user_id, last_synced_at, created_at, updated_at) VALUES (?, ?, NULL, ?, ?)",
        )
        .bind(test_pubkey.to_hex())
        .bind(test_user_id)
        .bind(test_timestamp)
        .bind(test_timestamp)
        .execute(&pool)
        .await
        .unwrap();

        let row: SqliteRow = sqlx::query("SELECT * FROM accounts WHERE pubkey = ?")
            .bind(test_pubkey.to_hex())
            .fetch_one(&pool)
            .await
            .unwrap();

        let account_row = AccountRow::from_row(&row).unwrap();

        assert_eq!(account_row.pubkey, test_pubkey);
        assert_eq!(account_row.user_id, test_user_id);
        assert!(account_row.last_synced_at.is_none());
        assert_eq!(account_row.created_at.timestamp_millis(), test_timestamp);
        assert_eq!(account_row.updated_at.timestamp_millis(), test_timestamp);
    }

    #[tokio::test]
    async fn test_account_row_from_row_invalid_pubkey() {
        let pool = setup_test_db().await;

        let invalid_pubkeys = [
            "not_a_pubkey",
            "too_short",
            "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz", // Invalid hex
            "",
        ];

        let test_timestamp = chrono::Utc::now().timestamp_millis();

        for (i, invalid_pubkey) in invalid_pubkeys.iter().enumerate() {
            let test_user_id = (i + 1) as i64;

            // Insert invalid pubkey
            sqlx::query(
                "INSERT INTO accounts (pubkey, user_id, created_at, updated_at) VALUES (?, ?, ?, ?)",
            )
            .bind(invalid_pubkey)
            .bind(test_user_id)
            .bind(test_timestamp)
            .bind(test_timestamp)
            .execute(&pool)
            .await
            .unwrap();

            // Test from_row implementation should fail
            let row: SqliteRow = sqlx::query("SELECT * FROM accounts WHERE pubkey = ?")
                .bind(invalid_pubkey)
                .fetch_one(&pool)
                .await
                .unwrap();

            let result = AccountRow::from_row(&row);
            assert!(result.is_err());

            if let Err(sqlx::Error::ColumnDecode { index, .. }) = result {
                assert_eq!(index, "pubkey");
            } else {
                panic!("Expected ColumnDecode error for pubkey");
            }

            // Clean up
            sqlx::query("DELETE FROM accounts WHERE pubkey = ?")
                .bind(invalid_pubkey)
                .execute(&pool)
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn test_account_row_from_row_invalid_timestamps() {
        let pool = setup_test_db().await;

        let test_pubkey = nostr_sdk::Keys::generate().public_key();
        let test_user_id = 777i64;
        let valid_timestamp = chrono::Utc::now().timestamp_millis();
        let invalid_timestamp = i64::MAX; // This will be too large for DateTime conversion

        // Test invalid last_synced_at timestamp
        sqlx::query(
            "INSERT INTO accounts (pubkey, user_id, last_synced_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(test_pubkey.to_hex())
        .bind(test_user_id)
        .bind(invalid_timestamp)
        .bind(valid_timestamp)
        .bind(valid_timestamp)
        .execute(&pool)
        .await
        .unwrap();

        let row: SqliteRow = sqlx::query("SELECT * FROM accounts WHERE pubkey = ?")
            .bind(test_pubkey.to_hex())
            .fetch_one(&pool)
            .await
            .unwrap();

        let result = AccountRow::from_row(&row);
        assert!(result.is_err());

        if let Err(sqlx::Error::ColumnDecode { index, .. }) = result {
            assert_eq!(index, "last_synced_at");
        } else {
            panic!("Expected ColumnDecode error for last_synced_at timestamp");
        }

        // Clean up and test invalid created_at timestamp
        sqlx::query("DELETE FROM accounts WHERE pubkey = ?")
            .bind(test_pubkey.to_hex())
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO accounts (pubkey, user_id, last_synced_at, created_at, updated_at) VALUES (?, ?, NULL, ?, ?)",
        )
        .bind(test_pubkey.to_hex())
        .bind(test_user_id)
        .bind(invalid_timestamp)
        .bind(valid_timestamp)
        .execute(&pool)
        .await
        .unwrap();

        let row: SqliteRow = sqlx::query("SELECT * FROM accounts WHERE pubkey = ?")
            .bind(test_pubkey.to_hex())
            .fetch_one(&pool)
            .await
            .unwrap();

        let result = AccountRow::from_row(&row);
        assert!(result.is_err());

        if let Err(sqlx::Error::ColumnDecode { index, .. }) = result {
            assert_eq!(index, "created_at");
        } else {
            panic!("Expected ColumnDecode error for created_at timestamp");
        }

        // Clean up and test invalid updated_at timestamp
        sqlx::query("DELETE FROM accounts WHERE pubkey = ?")
            .bind(test_pubkey.to_hex())
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO accounts (pubkey, user_id, last_synced_at, created_at, updated_at) VALUES (?, ?, NULL, ?, ?)",
        )
        .bind(test_pubkey.to_hex())
        .bind(test_user_id)
        .bind(valid_timestamp)
        .bind(invalid_timestamp)
        .execute(&pool)
        .await
        .unwrap();

        let row: SqliteRow = sqlx::query("SELECT * FROM accounts WHERE pubkey = ?")
            .bind(test_pubkey.to_hex())
            .fetch_one(&pool)
            .await
            .unwrap();

        let result = AccountRow::from_row(&row);
        assert!(result.is_err());

        if let Err(sqlx::Error::ColumnDecode { index, .. }) = result {
            assert_eq!(index, "updated_at");
        } else {
            panic!("Expected ColumnDecode error for updated_at timestamp");
        }
    }

    #[tokio::test]
    async fn test_save_account_success() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create test account
        let test_pubkey = nostr_sdk::Keys::generate().public_key();
        let test_user_id = 42i64;
        let test_last_synced = Some(chrono::Utc::now());
        let test_created_at = chrono::Utc::now();
        let test_updated_at = chrono::Utc::now();

        let account = Account {
            id: Some(1), // Will be overridden by database auto-increment
            pubkey: test_pubkey,
            user_id: test_user_id,
            last_synced_at: test_last_synced,
            created_at: test_created_at,
            updated_at: test_updated_at,
        };

        // Test save_account
        let result = account.save(&whitenoise.database).await;
        assert!(result.is_ok());

        // Test that we can load it back (this verifies it was saved correctly)
        let loaded_account = Account::find_by_pubkey(&test_pubkey, &whitenoise.database).await;
        assert!(loaded_account.is_ok());

        let loaded = loaded_account.unwrap();
        assert_eq!(loaded.pubkey, test_pubkey);
        assert_eq!(loaded.user_id, test_user_id);
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
    async fn test_save_account_with_null_last_synced() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        let test_pubkey = nostr_sdk::Keys::generate().public_key();
        let test_user_id = 123i64;
        let test_created_at = chrono::Utc::now();
        let test_updated_at = chrono::Utc::now();

        let account = Account {
            id: Some(1),
            pubkey: test_pubkey,
            user_id: test_user_id,
            last_synced_at: None, // Test with None
            created_at: test_created_at,
            updated_at: test_updated_at,
        };

        let result = account.save(&whitenoise.database).await;
        assert!(result.is_ok());

        // Verify it was saved correctly by loading it back
        let loaded_account = Account::find_by_pubkey(&test_pubkey, &whitenoise.database).await;
        assert!(loaded_account.is_ok());

        let loaded = loaded_account.unwrap();
        assert!(loaded.last_synced_at.is_none());
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
    async fn test_load_account_not_found() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Try to load a non-existent account
        let non_existent_pubkey = nostr_sdk::Keys::generate().public_key();
        let result = Account::find_by_pubkey(&non_existent_pubkey, &whitenoise.database).await;

        assert!(result.is_err());
        if let Err(WhitenoiseError::AccountNotFound) = result {
            // Expected error
        } else {
            panic!("Expected AccountNotFound error");
        }
    }

    #[tokio::test]
    async fn test_save_and_load_account_roundtrip() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create test account with all fields
        let test_pubkey = nostr_sdk::Keys::generate().public_key();
        let test_user_id = 555i64;
        let test_last_synced = Some(chrono::Utc::now());
        let test_created_at = chrono::Utc::now();
        let test_updated_at = chrono::Utc::now();

        let original_account = Account {
            id: Some(1),
            pubkey: test_pubkey,
            user_id: test_user_id,
            last_synced_at: test_last_synced,
            created_at: test_created_at,
            updated_at: test_updated_at,
        };

        // Save the account
        let save_result = original_account.save(&whitenoise.database).await;
        assert!(save_result.is_ok());

        // Load the account back
        let loaded_account = Account::find_by_pubkey(&test_pubkey, &whitenoise.database).await;
        assert!(loaded_account.is_ok());

        let account = loaded_account.unwrap();

        // Verify all fields match (except id which is set by database)
        assert_eq!(account.pubkey, original_account.pubkey);
        assert_eq!(account.user_id, original_account.user_id);
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

    // Tests for follows method
    #[tokio::test]
    async fn test_follows_multiple_followers() {
        use crate::whitenoise::users::User;

        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create test account
        let account_pubkey = nostr_sdk::Keys::generate().public_key();
        let account_user_id = 1i64;
        let test_timestamp = chrono::Utc::now();

        let account = Account {
            id: Some(1),
            pubkey: account_pubkey,
            user_id: account_user_id,
            last_synced_at: None,
            created_at: test_timestamp,
            updated_at: test_timestamp,
        };

        // Save the account first
        account.save(&whitenoise.database).await.unwrap();

        // Create test users that will be followers
        let mut test_users = Vec::new();
        let user_metadata_vec = vec![
            nostr_sdk::Metadata::new()
                .name("Alice")
                .display_name("Alice Wonderland")
                .about("The first user"),
            nostr_sdk::Metadata::new()
                .name("Bob")
                .display_name("Bob Builder")
                .about("The second user"),
            nostr_sdk::Metadata::new()
                .name("Charlie")
                .display_name("Charlie Chaplin")
                .about("The third user"),
        ];

        for metadata in user_metadata_vec.iter() {
            let user_pubkey = nostr_sdk::Keys::generate().public_key();
            let user = User {
                id: None, // Will be set by database
                pubkey: user_pubkey,
                metadata: metadata.clone(),
                created_at: test_timestamp,
                updated_at: test_timestamp,
            };

            // Save user first
            user.save(&whitenoise.database).await.unwrap();

            test_users.push((user_pubkey, metadata.clone()));
        }

        // Now manually insert the account_follows relationships
        // First we need to get the actual account ID and user IDs from the database
        let saved_account = Account::find_by_pubkey(&account_pubkey, &whitenoise.database)
            .await
            .unwrap();

        for (user_pubkey, _) in &test_users {
            let saved_user = User::find_by_pubkey(user_pubkey, &whitenoise.database)
                .await
                .unwrap();

            // Insert into account_follows table
            sqlx::query(
                "INSERT INTO account_follows (account_id, user_id, created_at, updated_at) VALUES (?, ?, ?, ?)"
            )
            .bind(saved_account.id)
            .bind(saved_user.id)
            .bind(test_timestamp.timestamp_millis())
            .bind(test_timestamp.timestamp_millis())
            .execute(&whitenoise.database.pool)
            .await
            .unwrap();
        }

        // Test follows
        let followers = saved_account.follows(&whitenoise.database).await.unwrap();

        // Verify we got all followers
        assert_eq!(followers.len(), 3);

        // Verify the followers match our test users
        for (expected_pubkey, expected_metadata) in &test_users {
            let follower = followers.iter().find(|f| &f.pubkey == expected_pubkey);
            assert!(
                follower.is_some(),
                "Expected follower with pubkey {} not found",
                expected_pubkey
            );

            let follower = follower.unwrap();
            assert_eq!(follower.metadata.name, expected_metadata.name);
            assert_eq!(
                follower.metadata.display_name,
                expected_metadata.display_name
            );
            assert_eq!(follower.metadata.about, expected_metadata.about);
        }
    }

    #[tokio::test]
    async fn test_follows_no_followers() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create test account
        let account_pubkey = nostr_sdk::Keys::generate().public_key();
        let account_user_id = 1i64;
        let test_timestamp = chrono::Utc::now();

        let account = Account {
            id: Some(1),
            pubkey: account_pubkey,
            user_id: account_user_id,
            last_synced_at: None,
            created_at: test_timestamp,
            updated_at: test_timestamp,
        };

        // Save the account
        account.save(&whitenoise.database).await.unwrap();
        let saved_account = Account::find_by_pubkey(&account_pubkey, &whitenoise.database)
            .await
            .unwrap();

        // Test follows with no followers
        let followers = saved_account.follows(&whitenoise.database).await.unwrap();

        // Verify empty result
        assert_eq!(followers.len(), 0);
    }

    #[tokio::test]
    async fn test_follows_single_follower() {
        use crate::whitenoise::users::User;

        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create test account
        let account_pubkey = nostr_sdk::Keys::generate().public_key();
        let account_user_id = 1i64;
        let test_timestamp = chrono::Utc::now();

        let account = Account {
            id: Some(1),
            pubkey: account_pubkey,
            user_id: account_user_id,
            last_synced_at: None,
            created_at: test_timestamp,
            updated_at: test_timestamp,
        };

        // Save the account
        account.save(&whitenoise.database).await.unwrap();

        // Create a single test user
        let user_pubkey = nostr_sdk::Keys::generate().public_key();
        let user_metadata = nostr_sdk::Metadata::new()
            .name("SingleUser")
            .display_name("Single User")
            .about("The only follower");

        let user = User {
            id: None,
            pubkey: user_pubkey,
            metadata: user_metadata.clone(),
            created_at: test_timestamp,
            updated_at: test_timestamp,
        };

        // Save user
        user.save(&whitenoise.database).await.unwrap();

        // Get the saved account and user with their database IDs
        let saved_account = Account::find_by_pubkey(&account_pubkey, &whitenoise.database)
            .await
            .unwrap();
        let saved_user = User::find_by_pubkey(&user_pubkey, &whitenoise.database)
            .await
            .unwrap();

        // Insert the follower relationship
        sqlx::query(
            "INSERT INTO account_follows (account_id, user_id, created_at, updated_at) VALUES (?, ?, ?, ?)"
        )
        .bind(saved_account.id)
        .bind(saved_user.id)
        .bind(test_timestamp.timestamp_millis())
        .bind(test_timestamp.timestamp_millis())
        .execute(&whitenoise.database.pool)
        .await
        .unwrap();

        // Test follows
        let followers = saved_account.follows(&whitenoise.database).await.unwrap();

        // Verify single follower
        assert_eq!(followers.len(), 1);
        let follower = &followers[0];
        assert_eq!(follower.pubkey, user_pubkey);
        assert_eq!(follower.metadata.name, user_metadata.name);
        assert_eq!(follower.metadata.display_name, user_metadata.display_name);
        assert_eq!(follower.metadata.about, user_metadata.about);
    }

    #[tokio::test]
    async fn test_follows_with_complex_user_metadata() {
        use crate::whitenoise::users::User;
        use nostr_sdk::prelude::Url;

        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create test account
        let account_pubkey = nostr_sdk::Keys::generate().public_key();
        let account_user_id = 1i64;
        let test_timestamp = chrono::Utc::now();

        let account = Account {
            id: Some(1),
            pubkey: account_pubkey,
            user_id: account_user_id,
            last_synced_at: None,
            created_at: test_timestamp,
            updated_at: test_timestamp,
        };

        account.save(&whitenoise.database).await.unwrap();

        // Create a user with complex metadata
        let user_pubkey = nostr_sdk::Keys::generate().public_key();
        let user_metadata = nostr_sdk::Metadata::new()
            .name("ComplexUser")
            .display_name("Complex User Display")
            .about("A user with comprehensive metadata including special characters: 🚀 and emojis")
            .picture(Url::parse("https://example.com/avatar.jpg").unwrap())
            .banner(Url::parse("https://example.com/banner.jpg").unwrap())
            .nip05("complex@example.com")
            .lud06("lnurl1dp68gurn8ghj7urp0v4kxvern9eehqurfdcsk6arpdd5kuemmduhxcmmrdaehgu3wd3skuep0dejhctnwda3kxvd09eszuekd0v8rqnrpwcxk7trj0ae8gmmwv9unx2txvg6xqmwpwejkcmmfd9c");

        let user = User {
            id: None,
            pubkey: user_pubkey,
            metadata: user_metadata.clone(),
            created_at: test_timestamp,
            updated_at: test_timestamp,
        };

        user.save(&whitenoise.database).await.unwrap();

        // Create the follower relationship
        let saved_account = Account::find_by_pubkey(&account_pubkey, &whitenoise.database)
            .await
            .unwrap();
        let saved_user = User::find_by_pubkey(&user_pubkey, &whitenoise.database)
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO account_follows (account_id, user_id, created_at, updated_at) VALUES (?, ?, ?, ?)"
        )
        .bind(saved_account.id)
        .bind(saved_user.id)
        .bind(test_timestamp.timestamp_millis())
        .bind(test_timestamp.timestamp_millis())
        .execute(&whitenoise.database.pool)
        .await
        .unwrap();

        // Test follows
        let followers = saved_account.follows(&whitenoise.database).await.unwrap();

        // Verify complex metadata is preserved
        assert_eq!(followers.len(), 1);
        let follower = &followers[0];
        assert_eq!(follower.pubkey, user_pubkey);
        assert_eq!(follower.metadata.name, user_metadata.name);
        assert_eq!(follower.metadata.display_name, user_metadata.display_name);
        assert_eq!(follower.metadata.about, user_metadata.about);
        assert_eq!(follower.metadata.picture, user_metadata.picture);
        assert_eq!(follower.metadata.banner, user_metadata.banner);
        assert_eq!(follower.metadata.nip05, user_metadata.nip05);
        assert_eq!(follower.metadata.lud06, user_metadata.lud06);
    }

    #[tokio::test]
    async fn test_follows_account_with_invalid_id() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create an account with an invalid ID that doesn't exist in the database
        let fake_account = Account {
            id: Some(99999), // Non-existent ID
            pubkey: nostr_sdk::Keys::generate().public_key(),
            user_id: 1,
            last_synced_at: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        // Test follows with non-existent account
        let result = fake_account.follows(&whitenoise.database).await;

        // Should return empty list rather than error since no followers exist
        assert!(result.is_ok());
        let followers = result.unwrap();
        assert_eq!(followers.len(), 0);
    }

    #[tokio::test]
    async fn test_follows_ordering_consistency() {
        use crate::whitenoise::users::User;

        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create test account
        let account_pubkey = nostr_sdk::Keys::generate().public_key();
        let test_timestamp = chrono::Utc::now();

        let account = Account {
            id: Some(1),
            pubkey: account_pubkey,
            user_id: 1,
            last_synced_at: None,
            created_at: test_timestamp,
            updated_at: test_timestamp,
        };

        account.save(&whitenoise.database).await.unwrap();

        // Create multiple users with predictable names
        let user_names = vec!["Alpha", "Beta", "Charlie", "Delta", "Echo"];
        let mut test_users = Vec::new();

        for name in user_names {
            let user_pubkey = nostr_sdk::Keys::generate().public_key();
            let user_metadata = nostr_sdk::Metadata::new().name(name);

            let user = User {
                id: None,
                pubkey: user_pubkey,
                metadata: user_metadata,
                created_at: test_timestamp,
                updated_at: test_timestamp,
            };

            user.save(&whitenoise.database).await.unwrap();
            test_users.push(user_pubkey);
        }

        // Create follower relationships
        let saved_account = Account::find_by_pubkey(&account_pubkey, &whitenoise.database)
            .await
            .unwrap();
        for user_pubkey in &test_users {
            let saved_user = User::find_by_pubkey(user_pubkey, &whitenoise.database)
                .await
                .unwrap();

            sqlx::query(
                "INSERT INTO account_follows (account_id, user_id, created_at, updated_at) VALUES (?, ?, ?, ?)"
            )
            .bind(saved_account.id)
            .bind(saved_user.id)
            .bind(test_timestamp.timestamp_millis())
            .bind(test_timestamp.timestamp_millis())
            .execute(&whitenoise.database.pool)
            .await
            .unwrap();
        }

        // Test follows multiple times to ensure consistent ordering
        let followers1 = saved_account.follows(&whitenoise.database).await.unwrap();
        let followers2 = saved_account.follows(&whitenoise.database).await.unwrap();

        assert_eq!(followers1.len(), 5);
        assert_eq!(followers2.len(), 5);

        // Results should be consistent between calls
        for (i, follower) in followers1.iter().enumerate() {
            assert_eq!(follower.pubkey, followers2[i].pubkey);
            assert_eq!(follower.metadata.name, followers2[i].metadata.name);
        }
    }

    #[tokio::test]
    async fn test_first_account() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Should return None when no accounts exist
        let account = Account::first(&whitenoise.database).await.unwrap();
        assert!(account.is_none());

        // If there's a single account, it should return that account
        let test_keys = nostr_sdk::Keys::generate();
        User::find_or_create_by_pubkey(&test_keys.public_key(), &whitenoise.database)
            .await
            .unwrap();
        let (test_account, _) = Account::new(&whitenoise, Some(test_keys)).await.unwrap();
        test_account.save(&whitenoise.database).await.unwrap();
        let account = Account::first(&whitenoise.database).await.unwrap();
        assert!(account.is_some());
        assert_eq!(account.unwrap().pubkey, test_account.pubkey);

        // If there's more than one account, it should still return the first one
        let test_keys2 = nostr_sdk::Keys::generate();
        User::find_or_create_by_pubkey(&test_keys2.public_key(), &whitenoise.database)
            .await
            .unwrap();
        let (test_account2, _) = Account::new(&whitenoise, Some(test_keys2)).await.unwrap();
        test_account2.save(&whitenoise.database).await.unwrap();
        let account2 = Account::first(&whitenoise.database).await.unwrap();
        assert!(account2.is_some());
        assert_eq!(account2.unwrap().pubkey, test_account.pubkey);

        // If that first account is deleted, it should return the second one
        test_account.delete(&whitenoise.database).await.unwrap();
        let account3 = Account::first(&whitenoise.database).await.unwrap();
        assert!(account3.is_some());
        assert_eq!(account3.unwrap().pubkey, test_account2.pubkey);

        // If all accounts are deleted, it should return None
        test_account2.delete(&whitenoise.database).await.unwrap();
        let account4 = Account::first(&whitenoise.database).await.unwrap();
        assert!(account4.is_none());
    }
}
