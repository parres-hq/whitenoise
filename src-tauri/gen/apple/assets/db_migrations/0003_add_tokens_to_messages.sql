-- Add tokens field to messages table
ALTER TABLE messages ADD COLUMN tokens JSON;  -- JSON array of SerializedToken values
