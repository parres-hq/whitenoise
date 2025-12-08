use std::collections::HashSet;

use chrono::{DateTime, Utc};
use mdk_core::prelude::{GroupId, message_types::Message};
use nostr_sdk::prelude::*;

use super::{Database, DatabaseError, utils::parse_timestamp};
use crate::nostr_manager::parser::SerializableToken;
use crate::whitenoise::{
    aggregated_message::AggregatedMessage,
    media_files::MediaFile,
    message_aggregator::{ChatMessage, ReactionSummary},
    utils::timestamp_to_datetime,
};

type Result<T> = std::result::Result<T, DatabaseError>;

#[derive(Debug)]
struct AggregatedMessageRow {
    pub id: i64,
    pub message_id: EventId,
    pub mls_group_id: GroupId,
    pub author: PublicKey,
    pub created_at: DateTime<Utc>,
    pub kind: Kind,
    pub content: String,
    pub tags: Tags,
    pub reply_to_id: Option<EventId>,
    pub deletion_event_id: Option<EventId>,
    pub content_tokens: Vec<SerializableToken>,
    pub reactions: ReactionSummary,
    pub media_attachments: Vec<MediaFile>,
}

impl<'r, R> sqlx::FromRow<'r, R> for AggregatedMessageRow
where
    R: sqlx::Row,
    &'r str: sqlx::ColumnIndex<R>,
    String: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
    i64: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
    Vec<u8>: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
{
    fn from_row(row: &'r R) -> std::result::Result<Self, sqlx::Error> {
        let id: i64 = row.try_get("id")?;

        // Convert message_id from hex string to EventId
        let message_id_hex: String = row.try_get("message_id")?;
        let message_id =
            EventId::from_hex(&message_id_hex).map_err(|e| sqlx::Error::ColumnDecode {
                index: "message_id".to_string(),
                source: Box::new(e),
            })?;

        // Convert mls_group_id from bytes to GroupId
        let mls_group_id_bytes: Vec<u8> = row.try_get("mls_group_id")?;
        let mls_group_id = GroupId::from_slice(&mls_group_id_bytes);

        // Convert author from hex string to PublicKey
        let author_hex: String = row.try_get("author")?;
        let author = PublicKey::from_hex(&author_hex).map_err(|e| sqlx::Error::ColumnDecode {
            index: "author".to_string(),
            source: Box::new(e),
        })?;

        // Convert created_at from milliseconds to DateTime<Utc>
        let created_at = parse_timestamp(row, "created_at")?;

        // Convert kind from i64 to Kind
        let kind_i64: i64 = row.try_get("kind")?;
        let kind = Kind::from(kind_i64 as u16);

        let content: String = row.try_get("content")?;

        // Deserialize tags from JSON string
        let tags_str: String = row.try_get("tags")?;
        let tags = serde_json::from_str(&tags_str).map_err(|e| sqlx::Error::ColumnDecode {
            index: "tags".to_string(),
            source: Box::new(e),
        })?;

        // Convert optional reply_to_id from hex string to EventId
        let reply_to_id = match row.try_get::<Option<String>, _>("reply_to_id")? {
            Some(hex) => Some(
                EventId::from_hex(&hex).map_err(|e| sqlx::Error::ColumnDecode {
                    index: "reply_to_id".to_string(),
                    source: Box::new(e),
                })?,
            ),
            None => None,
        };

        // Convert optional deletion_event_id from hex string to EventId
        let deletion_event_id = match row.try_get::<Option<String>, _>("deletion_event_id")? {
            Some(hex) => Some(
                EventId::from_hex(&hex).map_err(|e| sqlx::Error::ColumnDecode {
                    index: "deletion_event_id".to_string(),
                    source: Box::new(e),
                })?,
            ),
            None => None,
        };

        // Deserialize JSONB fields from JSON strings
        let content_tokens_str: String = row.try_get("content_tokens")?;
        let content_tokens =
            serde_json::from_str(&content_tokens_str).map_err(|e| sqlx::Error::ColumnDecode {
                index: "content_tokens".to_string(),
                source: Box::new(e),
            })?;

        let reactions_str: String = row.try_get("reactions")?;
        let reactions =
            serde_json::from_str(&reactions_str).map_err(|e| sqlx::Error::ColumnDecode {
                index: "reactions".to_string(),
                source: Box::new(e),
            })?;

        let media_attachments_str: String = row.try_get("media_attachments")?;
        let media_attachments = serde_json::from_str(&media_attachments_str).map_err(|e| {
            sqlx::Error::ColumnDecode {
                index: "media_attachments".to_string(),
                source: Box::new(e),
            }
        })?;

        Ok(Self {
            id,
            message_id,
            mls_group_id,
            author,
            created_at,
            kind,
            content,
            tags,
            reply_to_id,
            deletion_event_id,
            content_tokens,
            reactions,
            media_attachments,
        })
    }
}

impl AggregatedMessageRow {
    /// Convert database row to lightweight AggregatedMessage domain type
    fn into_aggregated_message(self) -> AggregatedMessage {
        AggregatedMessage {
            id: self.id,
            event_id: self.message_id,
            mls_group_id: self.mls_group_id,
            author: self.author,
            content: self.content,
            created_at: self.created_at,
            tags: self.tags,
        }
    }
}

impl AggregatedMessage {
    /// Count ALL events (kind 9, 7, 5) in cache for a group
    /// Used for sync checking: mdk.len() == cache.len()
    pub async fn count_by_group(group_id: &GroupId, database: &Database) -> Result<usize> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM aggregated_messages WHERE mls_group_id = ?")
                .bind(group_id.as_slice())
                .fetch_one(&database.pool)
                .await?;

        Ok(count as usize)
    }

    /// Get ALL event IDs (all kinds) for a group
    /// Used for incremental sync: filter out cached events
    pub async fn get_all_event_ids_by_group(
        group_id: &GroupId,
        database: &Database,
    ) -> Result<HashSet<String>> {
        let ids: Vec<String> =
            sqlx::query_scalar("SELECT message_id FROM aggregated_messages WHERE mls_group_id = ?")
                .bind(group_id.as_slice())
                .fetch_all(&database.pool)
                .await?;

        Ok(ids.into_iter().collect())
    }

    /// Fetch ONLY kind 9 messages for a group (main read path)
    /// This is what fetch_aggregated_messages_for_group calls
    ///
    /// Query uses covering index: idx_aggregated_messages_kind_group(kind, mls_group_id, created_at)
    pub async fn find_messages_by_group(
        group_id: &GroupId,
        database: &Database,
    ) -> Result<Vec<ChatMessage>> {
        let rows: Vec<AggregatedMessageRow> = sqlx::query_as(
            "SELECT * FROM aggregated_messages
             WHERE kind = 9 AND mls_group_id = ?
             ORDER BY created_at",
        )
        .bind(group_id.as_slice())
        .fetch_all(&database.pool)
        .await?;

        rows.into_iter().map(Self::row_to_chat_message).collect()
    }

    /// Save all events (kind 9, 7, 5) from sync in ONE transaction with single batch INSERT
    ///
    /// All events inserted in one batch - kind 9 gets full data, kind 7/5 get empty defaults
    /// Single pass - no UPDATE needed. This ensures atomicity: either all events are saved or none are
    pub async fn save_events(
        events: Vec<Message>,                 // All events (kind 9, 7, 5)
        processed_messages: Vec<ChatMessage>, // Processed kind 9 with aggregated data
        group_id: &GroupId,
        database: &Database,
    ) -> Result<()> {
        if events.is_empty() {
            return Ok(());
        }

        let mut tx = database.pool.begin().await?;

        // Build a map for quick lookup of processed messages
        let processed_map: std::collections::HashMap<String, &ChatMessage> = processed_messages
            .iter()
            .map(|msg| (msg.id.clone(), msg))
            .collect();

        // Empty defaults for kind 7/5 events
        let empty_tokens = Vec::<SerializableToken>::new();
        let empty_reactions = ReactionSummary::default();
        let empty_media = Vec::<MediaFile>::new();

        // Insert each event individually (SQLite doesn't support multi-value INSERT with JSONB)
        for message in &events {
            let created_at = timestamp_to_datetime(message.created_at).map_err(|_| {
                DatabaseError::InvalidTimestamp {
                    timestamp: message.created_at.as_u64() as i64,
                }
            })?;

            match message.kind {
                Kind::Custom(9) => {
                    // Kind 9: Get processed message data
                    let chat_msg = processed_map
                        .get(&message.id.to_string())
                        .ok_or_else(|| DatabaseError::Sqlx(sqlx::Error::RowNotFound))?;

                    sqlx::query(
                        "INSERT OR IGNORE INTO aggregated_messages
                         (message_id, mls_group_id, author, created_at, kind, content, tags,
                          reply_to_id, content_tokens, reactions, media_attachments)
                         VALUES (?, ?, ?, ?, 9, ?, ?, ?, ?, ?, ?)",
                    )
                    .bind(message.id.to_string())
                    .bind(group_id.as_slice())
                    .bind(message.pubkey.to_hex())
                    .bind(created_at.timestamp_millis())
                    .bind(&message.content)
                    .bind(serde_json::to_string(&message.tags)?)
                    .bind(chat_msg.reply_to_id.as_ref())
                    .bind(serde_json::to_string(&chat_msg.content_tokens)?)
                    .bind(serde_json::to_string(&chat_msg.reactions)?)
                    .bind(serde_json::to_string(&chat_msg.media_attachments)?)
                    .execute(&mut *tx)
                    .await?;
                }
                _ => {
                    // Kind 7/5: Use empty defaults
                    sqlx::query(
                        "INSERT OR IGNORE INTO aggregated_messages
                         (message_id, mls_group_id, author, created_at, kind, content, tags,
                          reply_to_id, content_tokens, reactions, media_attachments)
                         VALUES (?, ?, ?, ?, ?, ?, ?, NULL, ?, ?, ?)",
                    )
                    .bind(message.id.to_string())
                    .bind(group_id.as_slice())
                    .bind(message.pubkey.to_hex())
                    .bind(created_at.timestamp_millis())
                    .bind(u16::from(message.kind) as i64)
                    .bind(&message.content)
                    .bind(serde_json::to_string(&message.tags)?)
                    .bind(serde_json::to_string(&empty_tokens)?)
                    .bind(serde_json::to_string(&empty_reactions)?)
                    .bind(serde_json::to_string(&empty_media)?)
                    .execute(&mut *tx)
                    .await?;
                }
            }
        }

        tx.commit().await?;
        Ok(())
    }

    /// Insert a single kind 9 message with full pre-aggregated data
    /// Used by event processor for real-time caching
    pub async fn insert_message(
        message: &ChatMessage,
        group_id: &GroupId,
        database: &Database,
    ) -> Result<()> {
        let created_at = timestamp_to_datetime(message.created_at).map_err(|_| {
            DatabaseError::InvalidTimestamp {
                timestamp: message.created_at.as_u64() as i64,
            }
        })?;

        sqlx::query(
            "INSERT INTO aggregated_messages
             (message_id, mls_group_id, author, created_at, kind, content, tags,
              reply_to_id, content_tokens, reactions, media_attachments)
             VALUES (?, ?, ?, ?, 9, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(message_id, mls_group_id) DO UPDATE SET
               content = excluded.content,
               tags = excluded.tags,
               reply_to_id = excluded.reply_to_id,
               content_tokens = excluded.content_tokens,
               reactions = excluded.reactions,
               media_attachments = excluded.media_attachments",
        )
        .bind(&message.id)
        .bind(group_id.as_slice())
        .bind(message.author.to_hex())
        .bind(created_at.timestamp_millis())
        .bind(&message.content)
        .bind(serde_json::to_string(&message.tags)?)
        .bind(&message.reply_to_id)
        .bind(serde_json::to_string(&message.content_tokens)?)
        .bind(serde_json::to_string(&message.reactions)?)
        .bind(serde_json::to_string(&message.media_attachments)?)
        .execute(&database.pool)
        .await?;

        Ok(())
    }

    /// Insert a kind 7 reaction event (audit trail)
    pub async fn insert_reaction(
        reaction: &Message,
        group_id: &GroupId,
        database: &Database,
    ) -> Result<()> {
        let created_at = timestamp_to_datetime(reaction.created_at).map_err(|_| {
            DatabaseError::InvalidTimestamp {
                timestamp: reaction.created_at.as_u64() as i64,
            }
        })?;

        let empty_tokens = Vec::<SerializableToken>::new();
        let empty_reactions = ReactionSummary::default();
        let empty_media = Vec::<MediaFile>::new();

        sqlx::query(
            "INSERT INTO aggregated_messages
             (message_id, mls_group_id, author, created_at, kind, content, tags,
              content_tokens, reactions, media_attachments)
             VALUES (?, ?, ?, ?, 7, ?, ?, ?, ?, ?)
             ON CONFLICT(message_id, mls_group_id) DO NOTHING",
        )
        .bind(reaction.id.to_string())
        .bind(group_id.as_slice())
        .bind(reaction.pubkey.to_hex())
        .bind(created_at.timestamp_millis())
        .bind(&reaction.content)
        .bind(serde_json::to_string(&reaction.tags)?)
        .bind(serde_json::to_string(&empty_tokens)?)
        .bind(serde_json::to_string(&empty_reactions)?)
        .bind(serde_json::to_string(&empty_media)?)
        .execute(&database.pool)
        .await?;

        Ok(())
    }

    /// Insert a kind 5 deletion event (audit trail)
    pub async fn insert_deletion(
        deletion: &Message,
        group_id: &GroupId,
        database: &Database,
    ) -> Result<()> {
        let created_at = timestamp_to_datetime(deletion.created_at).map_err(|_| {
            DatabaseError::InvalidTimestamp {
                timestamp: deletion.created_at.as_u64() as i64,
            }
        })?;

        let empty_tokens = Vec::<SerializableToken>::new();
        let empty_reactions = ReactionSummary::default();
        let empty_media = Vec::<MediaFile>::new();

        sqlx::query(
            "INSERT INTO aggregated_messages
             (message_id, mls_group_id, author, created_at, kind, content, tags,
              content_tokens, reactions, media_attachments)
             VALUES (?, ?, ?, ?, 5, '', ?, ?, ?, ?)
             ON CONFLICT(message_id, mls_group_id) DO NOTHING",
        )
        .bind(deletion.id.to_string())
        .bind(group_id.as_slice())
        .bind(deletion.pubkey.to_hex())
        .bind(created_at.timestamp_millis())
        .bind(serde_json::to_string(&deletion.tags)?)
        .bind(serde_json::to_string(&empty_tokens)?)
        .bind(serde_json::to_string(&empty_reactions)?)
        .bind(serde_json::to_string(&empty_media)?)
        .execute(&database.pool)
        .await?;

        Ok(())
    }

    /// Update a kind 9 message's reaction summary
    pub async fn update_reactions(
        message_id: &str,
        group_id: &GroupId,
        reactions: &ReactionSummary,
        database: &Database,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE aggregated_messages
             SET reactions = ?
             WHERE message_id = ? AND mls_group_id = ? AND kind = 9",
        )
        .bind(serde_json::to_string(reactions)?)
        .bind(message_id)
        .bind(group_id.as_slice())
        .execute(&database.pool)
        .await?;

        Ok(())
    }

    /// Mark a message or reaction as deleted
    pub async fn mark_deleted(
        message_id: &str,
        group_id: &GroupId,
        deletion_event_id: &str,
        database: &Database,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE aggregated_messages
             SET deletion_event_id = ?
             WHERE message_id = ? AND mls_group_id = ? AND kind IN (7, 9)",
        )
        .bind(deletion_event_id)
        .bind(message_id)
        .bind(group_id.as_slice())
        .execute(&database.pool)
        .await?;

        Ok(())
    }

    /// Delete ALL cached events for a group
    pub async fn delete_by_group(group_id: &GroupId, database: &Database) -> Result<()> {
        sqlx::query("DELETE FROM aggregated_messages WHERE mls_group_id = ?")
            .bind(group_id.as_slice())
            .execute(&database.pool)
            .await?;
        Ok(())
    }

    /// Find a cached message by ID (for updating with reactions/deletions)
    pub async fn find_by_id(
        message_id: &str,
        group_id: &GroupId,
        database: &Database,
    ) -> Result<Option<ChatMessage>> {
        let row: Option<AggregatedMessageRow> = sqlx::query_as(
            "SELECT * FROM aggregated_messages
             WHERE message_id = ? AND mls_group_id = ? AND kind = 9",
        )
        .bind(message_id)
        .bind(group_id.as_slice())
        .fetch_optional(&database.pool)
        .await?;

        row.map(Self::row_to_chat_message).transpose()
    }

    /// Find a cached reaction (kind 7) by its event ID
    /// Only returns reactions that haven't been deleted yet
    pub async fn find_reaction_by_id(
        message_id: &str,
        group_id: &GroupId,
        database: &Database,
    ) -> Result<Option<AggregatedMessage>> {
        let row: Option<AggregatedMessageRow> = sqlx::query_as(
            "SELECT * FROM aggregated_messages
             WHERE message_id = ? AND mls_group_id = ? AND kind = 7
               AND deletion_event_id IS NULL",
        )
        .bind(message_id)
        .bind(group_id.as_slice())
        .fetch_optional(&database.pool)
        .await
        .map_err(DatabaseError::Sqlx)?;

        Ok(row.map(AggregatedMessageRow::into_aggregated_message))
    }

    /// Find orphaned reactions targeting a specific message
    /// Returns reactions (kind 7) that reference the target message_id
    /// Uses json_each to properly parse the tags array
    pub async fn find_orphaned_reactions(
        message_id: &str,
        group_id: &GroupId,
        database: &Database,
    ) -> Result<Vec<AggregatedMessage>> {
        let rows: Vec<AggregatedMessageRow> = sqlx::query_as(
            "SELECT am.* FROM aggregated_messages am
             WHERE am.kind = 7
               AND am.mls_group_id = ?
               AND EXISTS (
                 SELECT 1 FROM json_each(am.tags) AS tag
                 WHERE json_extract(tag.value, '$[0]') = 'e'
                   AND json_extract(tag.value, '$[1]') = ?
               )",
        )
        .bind(group_id.as_slice())
        .bind(message_id)
        .fetch_all(&database.pool)
        .await
        .map_err(DatabaseError::Sqlx)?;

        Ok(rows
            .into_iter()
            .map(AggregatedMessageRow::into_aggregated_message)
            .collect())
    }

    /// Find orphaned deletions targeting a specific message
    /// Returns the event IDs of deletions (kind 5) that reference the target message_id
    /// Uses json_each to properly parse the tags array
    pub async fn find_orphaned_deletions(
        message_id: &str,
        group_id: &GroupId,
        database: &Database,
    ) -> Result<Vec<EventId>> {
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT am.message_id FROM aggregated_messages am
             WHERE am.kind = 5
               AND am.mls_group_id = ?
               AND EXISTS (
                 SELECT 1 FROM json_each(am.tags) AS tag
                 WHERE json_extract(tag.value, '$[0]') = 'e'
                   AND json_extract(tag.value, '$[1]') = ?
               )",
        )
        .bind(group_id.as_slice())
        .bind(message_id)
        .fetch_all(&database.pool)
        .await
        .map_err(DatabaseError::Sqlx)?;

        Ok(ids
            .into_iter()
            .filter_map(|id| EventId::from_hex(&id).ok())
            .collect())
    }

    /// Convert database row to ChatMessage
    fn row_to_chat_message(row: AggregatedMessageRow) -> Result<ChatMessage> {
        // Convert DateTime<Utc> to Timestamp (seconds)
        let created_at = Timestamp::from(row.created_at.timestamp() as u64);

        Ok(ChatMessage {
            id: row.message_id.to_string(),
            author: row.author,
            content: row.content,
            created_at,
            tags: row.tags,
            is_reply: row.reply_to_id.is_some(),
            reply_to_id: row.reply_to_id.map(|id| id.to_string()),
            is_deleted: row.deletion_event_id.is_some(),
            content_tokens: row.content_tokens,
            reactions: row.reactions,
            kind: row.kind.as_u16(),
            media_attachments: row.media_attachments,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::whitenoise::group_information::{GroupInformation, GroupType};
    use crate::whitenoise::test_utils::create_mock_whitenoise;
    use nostr_sdk::Keys;

    async fn setup_group(group_id: &GroupId, database: &Database) {
        // Create group_information record (required for foreign key constraint)
        GroupInformation::find_or_create_by_mls_group_id(
            group_id,
            Some(GroupType::Group),
            database,
        )
        .await
        .unwrap();
    }

    fn create_test_chat_message(seed: u8, author: PublicKey) -> ChatMessage {
        // Create a valid 64-character hex string by repeating a pattern
        let id = format!("{:0>64}", format!("{:x}", seed));

        ChatMessage {
            id,
            author,
            content: "Test message".to_string(),
            created_at: Timestamp::now(),
            tags: Tags::new(),
            is_reply: false,
            reply_to_id: None,
            is_deleted: false,
            content_tokens: vec![],
            reactions: ReactionSummary::default(),
            kind: 9,
            media_attachments: vec![],
        }
    }

    #[tokio::test]
    async fn test_count_by_group_empty() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let group_id = GroupId::from_slice(&[1; 32]);

        let count = AggregatedMessage::count_by_group(&group_id, &whitenoise.database)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_get_all_event_ids_empty() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let group_id = GroupId::from_slice(&[1; 32]);

        let ids = AggregatedMessage::get_all_event_ids_by_group(&group_id, &whitenoise.database)
            .await
            .unwrap();
        assert!(ids.is_empty());
    }

    #[tokio::test]
    async fn test_find_messages_by_group_empty() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let group_id = GroupId::from_slice(&[1; 32]);

        let messages = AggregatedMessage::find_messages_by_group(&group_id, &whitenoise.database)
            .await
            .unwrap();
        assert!(messages.is_empty());
    }

    #[tokio::test]
    async fn test_insert_message() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let group_id = GroupId::from_slice(&[1; 32]);
        setup_group(&group_id, &whitenoise.database).await;

        let author = Keys::generate().public_key();
        let message = create_test_chat_message(1, author);

        // Insert message
        let result =
            AggregatedMessage::insert_message(&message, &group_id, &whitenoise.database).await;
        assert!(result.is_ok());

        // Verify it was inserted
        let count = AggregatedMessage::count_by_group(&group_id, &whitenoise.database)
            .await
            .unwrap();
        assert_eq!(count, 1);

        // Verify we can retrieve it
        let messages = AggregatedMessage::find_messages_by_group(&group_id, &whitenoise.database)
            .await
            .unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].id, message.id);
        assert_eq!(messages[0].content, message.content);
    }

    #[tokio::test]
    async fn test_insert_multiple_messages() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let group_id = GroupId::from_slice(&[2; 32]);
        setup_group(&group_id, &whitenoise.database).await;

        let author = Keys::generate().public_key();

        // Insert multiple messages
        let mut message_ids = vec![];
        for i in 1..=3 {
            let message = create_test_chat_message(i, author);
            message_ids.push(message.id.clone());
            AggregatedMessage::insert_message(&message, &group_id, &whitenoise.database)
                .await
                .unwrap();
        }

        // Verify count
        let count = AggregatedMessage::count_by_group(&group_id, &whitenoise.database)
            .await
            .unwrap();
        assert_eq!(count, 3);

        // Verify we can retrieve all messages
        let messages = AggregatedMessage::find_messages_by_group(&group_id, &whitenoise.database)
            .await
            .unwrap();
        assert_eq!(messages.len(), 3);

        // Verify event IDs
        let event_ids =
            AggregatedMessage::get_all_event_ids_by_group(&group_id, &whitenoise.database)
                .await
                .unwrap();
        assert_eq!(event_ids.len(), 3);
        for id in &message_ids {
            assert!(event_ids.contains(id));
        }
    }

    #[tokio::test]
    async fn test_mark_deleted_does_not_decrease_count() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let group_id = GroupId::from_slice(&[3; 32]);
        setup_group(&group_id, &whitenoise.database).await;

        let author = Keys::generate().public_key();

        // Insert a message
        let message = create_test_chat_message(10, author);
        AggregatedMessage::insert_message(&message, &group_id, &whitenoise.database)
            .await
            .unwrap();

        let count_before = AggregatedMessage::count_by_group(&group_id, &whitenoise.database)
            .await
            .unwrap();
        assert_eq!(count_before, 1);

        // Mark as deleted - need a valid 64-char hex ID
        let deletion_event_id = format!("{:0>64}", "abc123");
        AggregatedMessage::mark_deleted(
            &message.id,
            &group_id,
            &deletion_event_id,
            &whitenoise.database,
        )
        .await
        .unwrap();

        // Count should remain the same - mark_deleted doesn't remove the row
        let count_after = AggregatedMessage::count_by_group(&group_id, &whitenoise.database)
            .await
            .unwrap();
        assert_eq!(count_after, 1);

        // But the message should have deletion_event_id set
        let messages = AggregatedMessage::find_messages_by_group(&group_id, &whitenoise.database)
            .await
            .unwrap();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].is_deleted);
    }

    #[tokio::test]
    async fn test_delete_by_group_removes_all_events() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let group_id = GroupId::from_slice(&[4; 32]);
        setup_group(&group_id, &whitenoise.database).await;

        let author = Keys::generate().public_key();

        // Insert multiple messages
        let message1 = create_test_chat_message(20, author);
        AggregatedMessage::insert_message(&message1, &group_id, &whitenoise.database)
            .await
            .unwrap();

        let message2 = create_test_chat_message(21, author);
        AggregatedMessage::insert_message(&message2, &group_id, &whitenoise.database)
            .await
            .unwrap();

        let message3 = create_test_chat_message(22, author);
        AggregatedMessage::insert_message(&message3, &group_id, &whitenoise.database)
            .await
            .unwrap();

        // Verify count before deletion
        let count_before = AggregatedMessage::count_by_group(&group_id, &whitenoise.database)
            .await
            .unwrap();
        assert_eq!(count_before, 3);

        // Delete all events for the group
        AggregatedMessage::delete_by_group(&group_id, &whitenoise.database)
            .await
            .unwrap();

        // Count should now be zero
        let count_after = AggregatedMessage::count_by_group(&group_id, &whitenoise.database)
            .await
            .unwrap();
        assert_eq!(count_after, 0);

        // No messages should be found
        let messages = AggregatedMessage::find_messages_by_group(&group_id, &whitenoise.database)
            .await
            .unwrap();
        assert!(messages.is_empty());

        // No event IDs should be found
        let event_ids =
            AggregatedMessage::get_all_event_ids_by_group(&group_id, &whitenoise.database)
                .await
                .unwrap();
        assert!(event_ids.is_empty());
    }

    #[tokio::test]
    async fn test_delete_by_group_is_group_specific() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let group_id_1 = GroupId::from_slice(&[5; 32]);
        let group_id_2 = GroupId::from_slice(&[6; 32]);
        setup_group(&group_id_1, &whitenoise.database).await;
        setup_group(&group_id_2, &whitenoise.database).await;

        let author = Keys::generate().public_key();

        // Insert message in group 1
        let message1 = create_test_chat_message(30, author);
        AggregatedMessage::insert_message(&message1, &group_id_1, &whitenoise.database)
            .await
            .unwrap();

        // Insert message in group 2
        let message2 = create_test_chat_message(31, author);
        AggregatedMessage::insert_message(&message2, &group_id_2, &whitenoise.database)
            .await
            .unwrap();

        // Delete group 1
        AggregatedMessage::delete_by_group(&group_id_1, &whitenoise.database)
            .await
            .unwrap();

        // Group 1 should be empty
        let count_1 = AggregatedMessage::count_by_group(&group_id_1, &whitenoise.database)
            .await
            .unwrap();
        assert_eq!(count_1, 0);

        // Group 2 should still have its message
        let count_2 = AggregatedMessage::count_by_group(&group_id_2, &whitenoise.database)
            .await
            .unwrap();
        assert_eq!(count_2, 1);
    }

    #[tokio::test]
    async fn test_update_reactions() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let group_id = GroupId::from_slice(&[7; 32]);
        setup_group(&group_id, &whitenoise.database).await;

        let author = Keys::generate().public_key();

        // Insert a message with empty reactions
        let message = create_test_chat_message(40, author);
        AggregatedMessage::insert_message(&message, &group_id, &whitenoise.database)
            .await
            .unwrap();

        // Update with reactions
        let mut reactions = ReactionSummary::default();
        reactions.by_emoji.insert(
            "👍".to_string(),
            crate::whitenoise::message_aggregator::EmojiReaction {
                emoji: "👍".to_string(),
                count: 2,
                users: vec![author, Keys::generate().public_key()],
            },
        );

        AggregatedMessage::update_reactions(
            &message.id,
            &group_id,
            &reactions,
            &whitenoise.database,
        )
        .await
        .unwrap();

        // Verify reactions were updated
        let messages = AggregatedMessage::find_messages_by_group(&group_id, &whitenoise.database)
            .await
            .unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].reactions.by_emoji.len(), 1);
        assert!(messages[0].reactions.by_emoji.contains_key("👍"));
    }
}
