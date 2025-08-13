use crate::whitenoise::app_settings::{AppSettings, ThemeMode};
use crate::whitenoise::error::WhitenoiseError;
use crate::Whitenoise;
use chrono::{DateTime, Utc};
use std::str::FromStr;

#[derive(Debug)]
struct AppSettingsRow {
    id: i64,
    theme_mode: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl<'r, R> sqlx::FromRow<'r, R> for AppSettingsRow
where
    R: sqlx::Row,
    &'r str: sqlx::ColumnIndex<R>,
    String: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
    i64: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
{
    fn from_row(row: &'r R) -> std::result::Result<Self, sqlx::Error> {
        let id: i64 = row.try_get("id")?;
        let theme_mode: String = row.try_get("theme_mode")?;
        let created_at_i64: i64 = row.try_get("created_at")?;
        let updated_at_i64: i64 = row.try_get("updated_at")?;

        let created_at = DateTime::from_timestamp_millis(created_at_i64).ok_or_else(|| {
            sqlx::Error::ColumnDecode {
                index: "created_at".to_string(),
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Invalid timestamp",
                )),
            }
        })?;

        let updated_at = DateTime::from_timestamp_millis(updated_at_i64).ok_or_else(|| {
            sqlx::Error::ColumnDecode {
                index: "updated_at".to_string(),
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Invalid timestamp",
                )),
            }
        })?;

        Ok(AppSettingsRow {
            id,
            theme_mode,
            created_at,
            updated_at,
        })
    }
}

impl AppSettingsRow {
    /// Converts an AppSettingsRow to AppSettings
    fn into_app_settings(self) -> Result<AppSettings, WhitenoiseError> {
        let theme_mode = ThemeMode::from_str(&self.theme_mode)
            .map_err(|e| WhitenoiseError::Configuration(format!("Invalid theme mode: {}", e)))?;

        Ok(AppSettings {
            id: self.id,
            theme_mode,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

impl AppSettings {
    /// Loads the app settings from the database
    pub(crate) async fn load(whitenoise: &Whitenoise) -> Result<AppSettings, WhitenoiseError> {
        let settings_row =
            sqlx::query_as::<_, AppSettingsRow>("SELECT * FROM app_settings WHERE id = 1")
                .fetch_one(&whitenoise.database.pool)
                .await
                .map_err(|_| {
                    WhitenoiseError::Configuration("App settings not found".to_string())
                })?;

        settings_row.into_app_settings()
    }

    /// Saves or updates the app settings in the database
    pub(crate) async fn save(settings: &AppSettings, whitenoise: &Whitenoise) -> Result<(), WhitenoiseError> {
        sqlx::query(
            "INSERT OR REPLACE INTO app_settings (id, theme_mode, created_at, updated_at) VALUES (?, ?, ?, ?)"
        )
        .bind(settings.id)
        .bind(settings.theme_mode.to_string())
        .bind(settings.created_at.timestamp_millis())
        .bind(settings.updated_at.timestamp_millis())
        .execute(&whitenoise.database.pool)
        .await
        .map_err(|e| WhitenoiseError::Database(e.into()))?;

        Ok(())
    }

    /// Updates just the theme mode in the app settings
    pub(crate) async fn update_theme_mode(theme_mode: ThemeMode, whitenoise: &Whitenoise) -> Result<(), WhitenoiseError> {
        sqlx::query("UPDATE app_settings SET theme_mode = ?, updated_at = ? WHERE id = 1")
            .bind(theme_mode.to_string())
            .bind(Utc::now().timestamp_millis())
            .execute(&whitenoise.database.pool)
            .await
            .map_err(|e| WhitenoiseError::Database(e.into()))?;

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

        // Create the app_settings table
        sqlx::query(
            "CREATE TABLE app_settings (
                id INTEGER PRIMARY KEY,
                theme_mode TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    #[tokio::test]
    async fn test_app_settings_row_from_row_valid_data() {
        let pool = setup_test_db().await;

        let test_id = 1i64;
        let test_theme_mode = "dark";
        let test_created_at = chrono::Utc::now().timestamp_millis();
        let test_updated_at = chrono::Utc::now().timestamp_millis();

        // Insert test data
        sqlx::query(
            "INSERT INTO app_settings (id, theme_mode, created_at, updated_at) VALUES (?, ?, ?, ?)",
        )
        .bind(test_id)
        .bind(test_theme_mode)
        .bind(test_created_at)
        .bind(test_updated_at)
        .execute(&pool)
        .await
        .unwrap();

        // Test from_row implementation
        let row: SqliteRow = sqlx::query("SELECT * FROM app_settings WHERE id = ?")
            .bind(test_id)
            .fetch_one(&pool)
            .await
            .unwrap();

        let app_settings_row = AppSettingsRow::from_row(&row).unwrap();

        assert_eq!(app_settings_row.id, test_id);
        assert_eq!(app_settings_row.theme_mode, test_theme_mode);
        assert_eq!(
            app_settings_row.created_at.timestamp_millis(),
            test_created_at
        );
        assert_eq!(
            app_settings_row.updated_at.timestamp_millis(),
            test_updated_at
        );
    }

    #[tokio::test]
    async fn test_app_settings_row_from_row_all_theme_modes() {
        let pool = setup_test_db().await;
        let timestamp = chrono::Utc::now().timestamp_millis();

        let test_cases = vec![
            ("light", ThemeMode::Light),
            ("dark", ThemeMode::Dark),
            ("system", ThemeMode::System),
        ];

        for (theme_str, expected_theme) in test_cases {
            // Insert test data
            sqlx::query("DELETE FROM app_settings")
                .execute(&pool)
                .await
                .unwrap();

            sqlx::query(
                "INSERT INTO app_settings (id, theme_mode, created_at, updated_at) VALUES (?, ?, ?, ?)",
            )
            .bind(1i64)
            .bind(theme_str)
            .bind(timestamp)
            .bind(timestamp)
            .execute(&pool)
            .await
            .unwrap();

            // Test from_row implementation
            let row: SqliteRow = sqlx::query("SELECT * FROM app_settings WHERE id = 1")
                .fetch_one(&pool)
                .await
                .unwrap();

            let app_settings_row = AppSettingsRow::from_row(&row).unwrap();
            let app_settings = app_settings_row.into_app_settings().unwrap();

            assert_eq!(app_settings.theme_mode, expected_theme);
        }
    }

    #[tokio::test]
    async fn test_app_settings_row_from_row_invalid_timestamp() {
        let pool = setup_test_db().await;

        // Insert invalid timestamp (negative value that can't be converted)
        sqlx::query(
            "INSERT INTO app_settings (id, theme_mode, created_at, updated_at) VALUES (?, ?, ?, ?)",
        )
        .bind(1i64)
        .bind("light")
        .bind(-1i64) // Invalid timestamp
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&pool)
        .await
        .unwrap();

        let row: SqliteRow = sqlx::query("SELECT * FROM app_settings WHERE id = 1")
            .fetch_one(&pool)
            .await
            .unwrap();

        let result = AppSettingsRow::from_row(&row);
        assert!(result.is_err());

        if let Err(sqlx::Error::ColumnDecode { index, .. }) = result {
            assert_eq!(index, "created_at");
        } else {
            panic!("Expected ColumnDecode error for created_at");
        }
    }

    #[tokio::test]
    async fn test_app_settings_row_into_app_settings_valid() {
        let timestamp = chrono::Utc::now();
        let app_settings_row = AppSettingsRow {
            id: 1,
            theme_mode: "light".to_string(),
            created_at: timestamp,
            updated_at: timestamp,
        };

        let app_settings = app_settings_row.into_app_settings().unwrap();

        assert_eq!(app_settings.id, 1);
        assert_eq!(app_settings.theme_mode, ThemeMode::Light);
        assert_eq!(app_settings.created_at, timestamp);
        assert_eq!(app_settings.updated_at, timestamp);
    }

    #[tokio::test]
    async fn test_app_settings_row_into_app_settings_invalid_theme() {
        let timestamp = chrono::Utc::now();
        let app_settings_row = AppSettingsRow {
            id: 1,
            theme_mode: "invalid_theme".to_string(),
            created_at: timestamp,
            updated_at: timestamp,
        };

        let result = app_settings_row.into_app_settings();
        assert!(result.is_err());

        if let Err(WhitenoiseError::Configuration(msg)) = result {
            assert!(msg.contains("Invalid theme mode"));
        } else {
            panic!("Expected Configuration error for invalid theme mode");
        }
    }

    #[test]
    fn test_app_settings_row_serialization() {
        let timestamp = chrono::Utc::now();
        let row = AppSettingsRow {
            id: 1,
            theme_mode: "dark".to_string(),
            created_at: timestamp,
            updated_at: timestamp,
        };

        // Test debug formatting doesn't panic
        let debug_str = format!("{:?}", row);
        assert!(debug_str.contains("AppSettingsRow"));
        assert!(debug_str.contains("dark"));
    }

    #[test]
    fn test_theme_mode_roundtrip() {
        let test_cases = vec![ThemeMode::Light, ThemeMode::Dark, ThemeMode::System];

        for theme in test_cases {
            let theme_str = theme.to_string();
            let parsed_theme = ThemeMode::from_str(&theme_str).unwrap();
            assert_eq!(theme, parsed_theme);
        }
    }

    // Note: Integration tests for load_app_settings, save_app_settings, and update_theme_mode
    // would require a full Whitenoise instance setup, which is typically done in integration tests
    // rather than unit tests. The database operations themselves are tested above.

    #[test]
    fn test_error_handling_coverage() {
        // Test that all error types can be created and formatted properly
        let config_error = WhitenoiseError::Configuration("Test error".to_string());
        let error_str = format!("{}", config_error);
        assert!(error_str.contains("Test error"));
    }
}
