-- Generic table to track events we've published to prevent infinite loops
CREATE TABLE published_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT NOT NULL UNIQUE
        CHECK (length(event_id) = 64 AND event_id GLOB '[0-9a-fA-F]*'), -- 64-char hex
    account_id INTEGER NOT NULL,          -- Who published it
    event_kind INTEGER NOT NULL,          -- Nostr event kind (3, 0, 10002, etc.)
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE CASCADE
);

-- Indexes for efficient lookups
CREATE INDEX idx_published_events_lookup ON published_events(event_id);
CREATE INDEX idx_published_events_account_kind ON published_events(account_id, event_kind);

-- Table to track processed events to ensure idempotency
CREATE TABLE processed_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT NOT NULL UNIQUE
        CHECK (length(event_id) = 64 AND event_id GLOB '[0-9a-fA-F]*'), -- 64-char hex
    event_kind INTEGER NOT NULL,          -- Nostr event kind
    processed_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Index for efficient lookups
CREATE INDEX idx_processed_events_lookup ON processed_events(event_id);
CREATE INDEX idx_processed_events_kind ON processed_events(event_kind);
