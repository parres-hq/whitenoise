-- Migration 0017: Add aggregated_messages table for caching ALL events
--
-- This migration implements database-backed caching for aggregated chat messages
-- to eliminate redundant recomputation on every fetch.
--
-- Key design decisions:
-- - Stores ALL event types (kind 9=messages, 7=reactions, 5=deletions, etc...) for complete audit trail
-- - Pre-aggregated data: Kind 9 messages store parsed tokens, reactions, media attachments
-- - Orphaned events: Kind 7/5 can arrive before their targets (handled in business logic)
--
--
CREATE TABLE aggregated_messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,

    -- Event identification (all kinds)
    message_id TEXT NOT NULL
        CHECK (length(message_id) = 64 AND message_id GLOB '[0-9a-fA-F]*'),
    mls_group_id BLOB NOT NULL,
    author TEXT NOT NULL
        CHECK (length(author) = 64 AND author GLOB '[0-9a-fA-F]*'),
    created_at INTEGER NOT NULL,  -- Unix timestamp in MILLISECONDS (i64)
    kind INTEGER NOT NULL,  -- 9 (message), 7 (reaction), 5 (deletion)

    -- Event content
    content TEXT NOT NULL DEFAULT '',
    tags JSONB NOT NULL,  -- Serialized via sqlx::types::Json<Tags>

    -- Kind 9 specific: Message metadata
    reply_to_id TEXT
        CHECK (reply_to_id IS NULL OR (length(reply_to_id) = 64 AND reply_to_id GLOB '[0-9a-fA-F]*')),
    deletion_event_id TEXT
        CHECK (deletion_event_id IS NULL OR (length(deletion_event_id) = 64 AND deletion_event_id GLOB '[0-9a-fA-F]*')),

    -- Kind 9 specific: Pre-aggregated data (defaults handled in Rust)
    content_tokens JSONB NOT NULL,  -- Vec<SerializableToken>
    reactions JSONB NOT NULL,  -- ReactionSummary
    media_attachments JSONB NOT NULL,  -- Vec<MediaFile>

    -- Constraints
    UNIQUE(message_id, mls_group_id),
    FOREIGN KEY (mls_group_id) REFERENCES group_information(mls_group_id) ON DELETE CASCADE
);

-- Indexes for efficient querying
CREATE INDEX idx_aggregated_messages_message_id ON aggregated_messages(message_id);

-- CRITICAL: Primary read path - covers "WHERE kind = 9 AND mls_group_id = ? ORDER BY created_at"
-- Index column order matters: kind first for WHERE clause filtering
CREATE INDEX idx_aggregated_messages_kind_group ON aggregated_messages(kind, mls_group_id, created_at);

-- For sync checking and queries that need all event types for a group
CREATE INDEX idx_aggregated_messages_group ON aggregated_messages(mls_group_id, created_at);
