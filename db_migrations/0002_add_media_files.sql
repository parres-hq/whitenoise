CREATE TABLE media_files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    mls_group_id BLOB NOT NULL,
    account_pubkey TEXT NOT NULL,
    file_path TEXT NOT NULL,
    blossom_url TEXT,
    file_hash TEXT NOT NULL,
    nostr_key TEXT,
    created_at INTEGER NOT NULL,
    file_metadata JSONB,
    FOREIGN KEY (account_pubkey) REFERENCES accounts(pubkey) ON DELETE CASCADE
);

-- Create indexes for faster lookups
CREATE INDEX idx_media_files_group ON media_files(mls_group_id, account_pubkey);
CREATE INDEX idx_media_files_created ON media_files(created_at);
CREATE INDEX idx_media_files_hash ON media_files(file_hash);
