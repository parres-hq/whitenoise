use crate::whitenoise::accounts::AccountSettings;
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
    last_synced_at: DateTime<Utc>,
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
        let last_synced_i64: i64 = row.try_get("last_synced_at")?;
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

        // Convert last_synced from i64 to Timestamp
        let last_synced_at = DateTime::from_timestamp_millis(last_synced_i64).unwrap();
        let created_at = DateTime::from_timestamp_millis(created_at_i64).unwrap();
        let updated_at = DateTime::from_timestamp_millis(updated_at_i64).unwrap();

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
