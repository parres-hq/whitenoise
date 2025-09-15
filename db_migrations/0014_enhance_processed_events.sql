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

-- Create indexes for efficient querying
CREATE INDEX idx_processed_events_event_kind ON processed_events(event_kind);
CREATE INDEX idx_processed_events_event_created_at ON processed_events(event_created_at);
CREATE INDEX idx_processed_events_kind_timestamp ON processed_events(event_kind, event_created_at);
CREATE INDEX idx_processed_events_author ON processed_events(author);
CREATE INDEX idx_processed_events_author_kind ON processed_events(author, event_kind);
CREATE INDEX idx_processed_events_author_kind_timestamp ON processed_events(author, event_kind, event_created_at);
