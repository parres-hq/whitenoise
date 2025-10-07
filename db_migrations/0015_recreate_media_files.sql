-- Migration 0015: Recreate media_files table with clean schema for media cache
--
-- This migration drops the experimental media_files table from migration 0002
-- and recreates it with a proper schema designed for:
-- 1. Group profile images (MIP-01)
-- 2. Future group media files (MIP-04)
--
-- Key features:
-- - Cross-account cache with reference tracking
-- - Normalized columns (no JSONB)
-- - Hash-based storage organization
-- - LRU eviction support via accessed_at

-- Drop old experimental media_files table
DROP TABLE IF EXISTS media_files;

-- Create new media_files table with clean schema
CREATE TABLE media_files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    mls_group_id BLOB NOT NULL,
    account_pubkey TEXT NOT NULL,
    file_path TEXT NOT NULL,
    file_hash TEXT NOT NULL,          -- Hex-encoded hash of ENCRYPTED data (image_hash for group images, per MIP-01)
    mime_type TEXT NOT NULL,          -- Canonical MIME type (e.g., "image/jpeg")
    media_type TEXT NOT NULL,         -- "group_image" or "chat_media"
    blossom_url TEXT,                 -- Optional: NULL for default server, set for custom
    nostr_key TEXT,                   -- Optional: For future MIP-04 cleanup
    dimensions TEXT,                  -- Optional: Image/video dimensions as "widthxheight" (e.g., "1920x1080")
    blurhash TEXT,                    -- Optional: BlurHash string for progressive loading
    created_at INTEGER NOT NULL,
    accessed_at INTEGER NOT NULL,     -- For LRU eviction
    FOREIGN KEY (account_pubkey) REFERENCES accounts(pubkey) ON DELETE CASCADE
);

-- Indexes for efficient queries
CREATE INDEX idx_media_files_group_hash ON media_files(mls_group_id, file_hash);
CREATE INDEX idx_media_files_account ON media_files(account_pubkey);
CREATE INDEX idx_media_files_created ON media_files(created_at);
CREATE INDEX idx_media_files_accessed ON media_files(accessed_at);
CREATE INDEX idx_media_files_type ON media_files(media_type);

-- Unique constraint: One row per (group, hash, account) combination
CREATE UNIQUE INDEX idx_media_files_unique ON media_files(mls_group_id, file_hash, account_pubkey);

