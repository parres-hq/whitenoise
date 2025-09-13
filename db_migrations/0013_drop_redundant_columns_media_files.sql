-- Migration: Drop nostr_key column from media_files
ALTER TABLE media_files
    DROP COLUMN nostr_key,
    DROP COLUMN blossom_url,
    DROP COLUMN file_path,
    DROP COLUMN file_metadata;
