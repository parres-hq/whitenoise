-- Migration: Drop nostr_key column from media_files
ALTER TABLE media_files DROP COLUMN nostr_key;
ALTER TABLE media_files DROP COLUMN blossom_url;
ALTER TABLE media_files DROP COLUMN file_path;
ALTER TABLE media_files DROP COLUMN file_metadata;
