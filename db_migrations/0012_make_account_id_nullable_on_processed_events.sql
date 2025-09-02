-- Make account_id nullable on processed_events table to support global event processing
-- Following the official SQLite 12-step ALTER TABLE procedure

-- Step 1: Disable foreign key constraints if enabled
PRAGMA foreign_keys=OFF;

-- Step 2: Create new table with desired schema

CREATE TABLE processed_events_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT NOT NULL
        CHECK (length(event_id) = 64 AND event_id GLOB '[0-9a-fA-F]*'), -- 64-char hex
    account_id INTEGER,                   -- Now nullable - which account was it processed for (NULL for global)
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE CASCADE,
    UNIQUE(event_id, account_id)          -- Each account can only process a specific event once (allows multiple NULL account_ids per event_id)
);

-- Step 3: Transfer content from old table to new table
INSERT INTO processed_events_new (id, event_id, account_id, created_at)
SELECT id, event_id, account_id, created_at FROM processed_events;

-- Step 4: Drop old table
DROP TABLE processed_events;

-- Step 5: Rename new table to original name
ALTER TABLE processed_events_new RENAME TO processed_events;

-- Step 6: Recreate indexes associated with the table
CREATE INDEX idx_processed_events_lookup ON processed_events(event_id);
CREATE INDEX idx_processed_events_account_id ON processed_events(account_id);

-- Step 7: Add partial unique index for global events (account_id=NULL) are unique per event_id
CREATE UNIQUE INDEX idx_processed_events_global_unique
ON processed_events(event_id)
WHERE account_id IS NULL;

-- Step 8: Verify foreign key constraints
PRAGMA foreign_key_check;

-- Step 9: Re-enable foreign key constraints
PRAGMA foreign_keys=ON;
