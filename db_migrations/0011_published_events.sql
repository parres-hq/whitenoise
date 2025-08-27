-- Generic table to track events we've published to prevent infinite loops
CREATE TABLE published_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT NOT NULL
        CHECK (length(event_id) = 64 AND event_id GLOB '[0-9a-fA-F]*'), -- 64-char hex
    account_id INTEGER NOT NULL,          -- Who published it
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE CASCADE,
    UNIQUE(event_id, account_id)          -- Each account can only publish a specific event once
);

-- Indexes for efficient lookups
CREATE INDEX idx_published_events_lookup ON published_events(event_id);
CREATE INDEX idx_published_events_account_id ON published_events(account_id);

-- Table to track processed events to ensure idempotency
CREATE TABLE processed_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT NOT NULL
        CHECK (length(event_id) = 64 AND event_id GLOB '[0-9a-fA-F]*'), -- 64-char hex
    account_id INTEGER NOT NULL,          -- Which account was it processed for?
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE CASCADE,
    UNIQUE(event_id, account_id)          -- Each account can only process a specific event once
);

-- Index for efficient lookups
CREATE INDEX idx_processed_events_lookup ON processed_events(event_id);
CREATE INDEX idx_processed_events_account_id ON processed_events(account_id);
