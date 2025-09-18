-- Enhance processed_events table with event timestamp, kind tracking, and author field
-- This replaces the previous approach of adding event_created_at to individual tables
-- Using simple ALTER TABLE ADD COLUMN approach

-- Add original Nostr event timestamp (milliseconds since Unix epoch)
-- NULL for legacy data where we don't have the original timestamp
ALTER TABLE processed_events ADD COLUMN event_created_at INTEGER DEFAULT NULL;

-- Add Nostr event kind (0, 3, 10002, etc.)
-- NULL for legacy data where we don't know the kind
ALTER TABLE processed_events ADD COLUMN event_kind INTEGER DEFAULT NULL;

-- Add author (Nostr public key hex) to track the original event publisher
-- NULL for legacy data where we don't have the author information
ALTER TABLE processed_events ADD COLUMN author TEXT DEFAULT NULL;

-- Create indexes for efficient querying based on actual query patterns

-- 1. Primary lookup for idempotency checks (event_id + account_id combinations)
CREATE INDEX idx_processed_events_event_account ON processed_events(event_id, account_id);

-- 2. Account-specific timestamp queries (account_id + event_kind for MAX queries)
CREATE INDEX idx_processed_events_account_kind_timestamp ON processed_events(account_id, event_kind, event_created_at);

-- 3. Global events with author filtering (author + event_kind for MAX queries)
CREATE INDEX idx_processed_events_author_kind_timestamp ON processed_events(author, event_kind, event_created_at);

-- 4. Global events without author filtering (account_id IS NULL + event_kind for MAX queries)
CREATE INDEX idx_processed_events_null_account_kind_timestamp ON processed_events(event_kind, event_created_at) WHERE account_id IS NULL;
