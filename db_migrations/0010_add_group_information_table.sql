-- Add group information table
CREATE TABLE group_information (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    mls_group_id BLOB NOT NULL UNIQUE,  -- Foreign key to the groups table, managed by nostr-mls
    group_type TEXT NOT NULL DEFAULT 'group',
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);
