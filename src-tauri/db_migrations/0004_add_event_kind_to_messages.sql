-- Add event_kind column to messages table
ALTER TABLE messages ADD COLUMN event_kind INTEGER;

-- Create index on event_kind column
CREATE INDEX idx_messages_event_kind ON messages(event_kind);
