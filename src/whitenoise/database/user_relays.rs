use super::DatabaseError;
use crate::whitenoise::relays::RelayType;
use chrono::{DateTime, Utc};

#[derive(Debug)]
pub(crate) struct UserRelayRow {
    // user_id is the ID of the user
    pub user_id: i64,
    // relay_id is the ID of the relay
    pub relay_id: i64,
    // relay_type is the type of the relay
    pub relay_type: RelayType,
    // created_at is the timestamp of the user relay creation
    pub created_at: DateTime<Utc>,
    // updated_at is the timestamp of the last update
    pub updated_at: DateTime<Utc>,
}

impl<'r, R> sqlx::FromRow<'r, R> for UserRelayRow
where
    R: sqlx::Row,
    &'r str: sqlx::ColumnIndex<R>,
    String: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
    i64: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
{
    fn from_row(row: &'r R) -> std::result::Result<Self, sqlx::Error> {
        let user_id: i64 = row.try_get("user_id")?;
        let relay_id: i64 = row.try_get("relay_id")?;
        let relay_type_str: String = row.try_get("relay_type")?;
        let created_at_i64: i64 = row.try_get("created_at")?;
        let updated_at_i64: i64 = row.try_get("updated_at")?;

        let relay_type = RelayType::from(relay_type_str);

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

        Ok(UserRelayRow {
            user_id,
            relay_id,
            relay_type,
            created_at,
            updated_at,
        })
    }
}
