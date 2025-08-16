use chrono::{DateTime, Utc};
use sqlx::Row;

/// Parses a timestamp column with flexible type handling for SQLite type affinity.
///
/// This function gracefully handles SQLite's type affinity by trying to parse
/// the column as both INTEGER (milliseconds since Unix epoch) and TEXT
/// (datetime string). This allows the application to work with mixed data
/// that may have been inserted using different methods.
///
/// # Arguments
/// * `row` - The database row to parse from
/// * `column_name` - Name of the timestamp column to parse
///
/// # Returns
/// * `Ok(DateTime<Utc>)` - Successfully parsed timestamp
/// * `Err(sqlx::Error)` - Column doesn't exist or couldn't be parsed as either format
///
/// # Examples
/// ```ignore
/// // Works with INTEGER timestamps (milliseconds)
/// let timestamp = parse_timestamp(&row, "created_at")?;
///
/// // Also works with TEXT timestamps ("2025-08-16 11:34:29")
/// let timestamp = parse_timestamp(&row, "updated_at")?;
/// ```
pub(crate) fn parse_timestamp<'r, R>(
    row: &'r R,
    column_name: &'r str,
) -> Result<DateTime<Utc>, sqlx::Error>
where
    R: Row,
    &'r str: sqlx::ColumnIndex<R>,
    i64: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
    String: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
{
    // Try INTEGER timestamp first (milliseconds)
    if let Ok(timestamp_ms) = row.try_get::<i64, _>(column_name) {
        return DateTime::from_timestamp_millis(timestamp_ms)
            .ok_or_else(|| create_column_decode_error(column_name, "Invalid timestamp value"));
    }

    // Fall back to TEXT datetime string
    if let Ok(datetime_str) = row.try_get::<String, _>(column_name) {
        let formatted_str = format!("{}+00:00", datetime_str);
        return formatted_str
            .parse::<DateTime<Utc>>()
            .map_err(|e| sqlx::Error::ColumnDecode {
                index: column_name.to_string(),
                source: Box::new(e),
            });
    }

    Err(create_column_decode_error(
        column_name,
        "Could not parse as INTEGER or DATETIME",
    ))
}

/// Helper function to create consistent ColumnDecode errors.
pub(crate) fn create_column_decode_error(column_name: &str, message: &str) -> sqlx::Error {
    sqlx::Error::ColumnDecode {
        index: column_name.to_string(),
        source: Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, Timelike};
    use sqlx::sqlite::{SqlitePool, SqliteRow};

    async fn setup_test_db() -> SqlitePool {
        let pool = SqlitePool::connect(":memory:").await.unwrap();

        // Create test table with mixed timestamp types
        sqlx::query(
            "CREATE TABLE test_timestamps (
                id INTEGER PRIMARY KEY,
                int_timestamp INTEGER,
                text_timestamp TEXT,
                invalid_int INTEGER,
                invalid_text TEXT
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    #[tokio::test]
    async fn test_parse_timestamp_integer_valid() {
        let pool = setup_test_db().await;
        let test_timestamp = chrono::Utc::now().timestamp_millis();

        sqlx::query("INSERT INTO test_timestamps (id, int_timestamp) VALUES (1, ?)")
            .bind(test_timestamp)
            .execute(&pool)
            .await
            .unwrap();

        let row: SqliteRow = sqlx::query("SELECT * FROM test_timestamps WHERE id = 1")
            .fetch_one(&pool)
            .await
            .unwrap();

        let result = parse_timestamp(&row, "int_timestamp");
        assert!(result.is_ok());

        let parsed_time = result.unwrap();
        assert_eq!(parsed_time.timestamp_millis(), test_timestamp);
    }

    #[tokio::test]
    async fn test_parse_timestamp_text_valid() {
        let pool = setup_test_db().await;

        sqlx::query("INSERT INTO test_timestamps (id, text_timestamp) VALUES (1, ?)")
            .bind("2025-08-16 11:34:29")
            .execute(&pool)
            .await
            .unwrap();

        let row: SqliteRow = sqlx::query("SELECT * FROM test_timestamps WHERE id = 1")
            .fetch_one(&pool)
            .await
            .unwrap();

        let result = parse_timestamp(&row, "text_timestamp");
        assert!(result.is_ok());

        let parsed_time = result.unwrap();
        assert_eq!(parsed_time.year(), 2025);
        assert_eq!(parsed_time.month(), 8);
        assert_eq!(parsed_time.day(), 16);
        assert_eq!(parsed_time.hour(), 11);
        assert_eq!(parsed_time.minute(), 34);
        assert_eq!(parsed_time.second(), 29);
    }

    #[tokio::test]
    async fn test_parse_timestamp_text_with_subseconds() {
        let pool = setup_test_db().await;

        sqlx::query("INSERT INTO test_timestamps (id, text_timestamp) VALUES (1, ?)")
            .bind("2025-08-16 11:34:29.123")
            .execute(&pool)
            .await
            .unwrap();

        let row: SqliteRow = sqlx::query("SELECT * FROM test_timestamps WHERE id = 1")
            .fetch_one(&pool)
            .await
            .unwrap();

        let result = parse_timestamp(&row, "text_timestamp");
        assert!(result.is_ok());

        let parsed_time = result.unwrap();
        assert_eq!(parsed_time.timestamp_subsec_millis(), 123);
    }

    #[tokio::test]
    async fn test_parse_timestamp_invalid_integer() {
        let pool = setup_test_db().await;

        // Use i64::MIN which should be invalid for DateTime::from_timestamp_millis
        sqlx::query("INSERT INTO test_timestamps (id, invalid_int) VALUES (1, ?)")
            .bind(i64::MIN)
            .execute(&pool)
            .await
            .unwrap();

        let row: SqliteRow = sqlx::query("SELECT * FROM test_timestamps WHERE id = 1")
            .fetch_one(&pool)
            .await
            .unwrap();

        let result = parse_timestamp(&row, "invalid_int");
        assert!(result.is_err());

        if let Err(sqlx::Error::ColumnDecode { index, .. }) = result {
            assert_eq!(index, "invalid_int");
        } else {
            panic!("Expected ColumnDecode error");
        }
    }

    #[tokio::test]
    async fn test_parse_timestamp_invalid_text() {
        let pool = setup_test_db().await;

        sqlx::query("INSERT INTO test_timestamps (id, invalid_text) VALUES (1, ?)")
            .bind("not a timestamp")
            .execute(&pool)
            .await
            .unwrap();

        let row: SqliteRow = sqlx::query("SELECT * FROM test_timestamps WHERE id = 1")
            .fetch_one(&pool)
            .await
            .unwrap();

        let result = parse_timestamp(&row, "invalid_text");
        assert!(result.is_err());

        if let Err(sqlx::Error::ColumnDecode { index, .. }) = result {
            assert_eq!(index, "invalid_text");
        } else {
            panic!("Expected ColumnDecode error");
        }
    }

    #[tokio::test]
    async fn test_parse_timestamp_nonexistent_column() {
        let pool = setup_test_db().await;

        sqlx::query("INSERT INTO test_timestamps (id) VALUES (1)")
            .execute(&pool)
            .await
            .unwrap();

        let row: SqliteRow = sqlx::query("SELECT * FROM test_timestamps WHERE id = 1")
            .fetch_one(&pool)
            .await
            .unwrap();

        let result = parse_timestamp(&row, "nonexistent_column");
        assert!(result.is_err());

        if let Err(sqlx::Error::ColumnDecode { index, .. }) = result {
            assert_eq!(index, "nonexistent_column");
        } else {
            panic!("Expected ColumnDecode error");
        }
    }

    #[tokio::test]
    async fn test_parse_timestamp_text_without_subseconds() {
        let pool = setup_test_db().await;

        sqlx::query("INSERT INTO test_timestamps (id, text_timestamp) VALUES (1, ?)")
            .bind("2025-08-16 11:34:29") // No subseconds
            .execute(&pool)
            .await
            .unwrap();

        let row: SqliteRow = sqlx::query("SELECT * FROM test_timestamps WHERE id = 1")
            .fetch_one(&pool)
            .await
            .unwrap();

        let result = parse_timestamp(&row, "text_timestamp");
        assert!(result.is_ok());

        let parsed_time = result.unwrap();
        assert_eq!(parsed_time.timestamp_subsec_millis(), 0); // Should be 0 when no subseconds
        assert_eq!(parsed_time.year(), 2025);
        assert_eq!(parsed_time.month(), 8);
        assert_eq!(parsed_time.day(), 16);
        assert_eq!(parsed_time.hour(), 11);
        assert_eq!(parsed_time.minute(), 34);
        assert_eq!(parsed_time.second(), 29);
    }

    #[tokio::test]
    async fn test_parse_timestamp_integer_subsecond_precision() {
        let pool = setup_test_db().await;

        // Test specific millisecond values
        let test_cases = [
            1755343067000, // Exact seconds (no subseconds)
            1755343067123, // 123 milliseconds
            1755343067456, // 456 milliseconds
            1755343067999, // 999 milliseconds (max)
        ];

        for (i, test_timestamp) in test_cases.iter().enumerate() {
            let id = i + 1;
            sqlx::query("INSERT INTO test_timestamps (id, int_timestamp) VALUES (?, ?)")
                .bind(id as i64)
                .bind(*test_timestamp)
                .execute(&pool)
                .await
                .unwrap();

            let row: SqliteRow = sqlx::query("SELECT * FROM test_timestamps WHERE id = ?")
                .bind(id as i64)
                .fetch_one(&pool)
                .await
                .unwrap();

            let result = parse_timestamp(&row, "int_timestamp");
            assert!(result.is_ok());

            let parsed_time = result.unwrap();
            assert_eq!(parsed_time.timestamp_millis(), *test_timestamp);

            // Verify subsecond precision is preserved
            let expected_subsec = (*test_timestamp % 1000) as u32;
            assert_eq!(parsed_time.timestamp_subsec_millis(), expected_subsec);
        }
    }

    #[tokio::test]
    async fn test_parse_timestamp_text_various_subsecond_formats() {
        let pool = setup_test_db().await;

        let test_cases = [
            ("2025-08-16 11:34:29", 0),       // No subseconds
            ("2025-08-16 11:34:29.1", 100),   // Single digit subseconds
            ("2025-08-16 11:34:29.12", 120),  // Two digit subseconds
            ("2025-08-16 11:34:29.123", 123), // Three digit subseconds
            ("2025-08-16 11:34:29.000", 0),   // Explicit zero subseconds
        ];

        for (i, (timestamp_str, expected_millis)) in test_cases.iter().enumerate() {
            let id = i + 1;

            // Clear previous data
            sqlx::query("DELETE FROM test_timestamps")
                .execute(&pool)
                .await
                .unwrap();

            sqlx::query("INSERT INTO test_timestamps (id, text_timestamp) VALUES (?, ?)")
                .bind(id as i64)
                .bind(timestamp_str)
                .execute(&pool)
                .await
                .unwrap();

            let row: SqliteRow = sqlx::query("SELECT * FROM test_timestamps WHERE id = ?")
                .bind(id as i64)
                .fetch_one(&pool)
                .await
                .unwrap();

            let result = parse_timestamp(&row, "text_timestamp");
            assert!(result.is_ok(), "Failed to parse: {}", timestamp_str);

            let parsed_time = result.unwrap();
            assert_eq!(
                parsed_time.timestamp_subsec_millis(),
                *expected_millis,
                "Subsecond mismatch for: {} (expected: {}, got: {})",
                timestamp_str,
                expected_millis,
                parsed_time.timestamp_subsec_millis()
            );
        }
    }

    #[tokio::test]
    async fn test_parse_timestamp_priority_integer_over_text() {
        let pool = setup_test_db().await;
        let test_timestamp = chrono::Utc::now().timestamp_millis();

        // Insert both integer and text values - integer should take priority
        sqlx::query(
            "INSERT INTO test_timestamps (id, int_timestamp, text_timestamp) VALUES (1, ?, ?)",
        )
        .bind(test_timestamp)
        .bind("2020-01-01 00:00:00") // Different date to verify integer is used
        .execute(&pool)
        .await
        .unwrap();

        let row: SqliteRow = sqlx::query("SELECT * FROM test_timestamps WHERE id = 1")
            .fetch_one(&pool)
            .await
            .unwrap();

        // When both are available, should parse as integer (the current timestamp, not 2020)
        let result = parse_timestamp(&row, "int_timestamp");
        assert!(result.is_ok());

        let parsed_time = result.unwrap();
        assert_eq!(parsed_time.timestamp_millis(), test_timestamp);
        assert!(parsed_time.year() > 2020); // Should be recent timestamp, not 2020
    }
}
