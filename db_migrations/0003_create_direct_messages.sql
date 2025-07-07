CREATE TABLE direct_messages (
    account_pubkey TEXT NOT NULL,
    sender_pubkey TEXT NOT NULL,
    content TEXT NOT NULL,
    tags JSONB,
    created_at INTEGER NOT NULL
);
