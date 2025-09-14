-- Add event_created_at columns to track original event timestamps
-- This prevents stale events from overwriting newer data during sync operations

-- Add event_created_at to users table for metadata events
-- NULL indicates we don't have the original event timestamp (legacy data)
ALTER TABLE users ADD COLUMN event_created_at INTEGER DEFAULT NULL;

-- Add event_created_at to user_relays table for relay list events
-- NULL indicates we don't have the original event timestamp (legacy data)
ALTER TABLE user_relays ADD COLUMN event_created_at INTEGER DEFAULT NULL;
