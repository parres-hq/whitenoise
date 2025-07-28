-- Step 1: Create a new temporary table with the updated schema
CREATE TABLE accounts_new (
    pubkey TEXT PRIMARY KEY,
    settings JSONB NOT NULL,
    nip65_relays TEXT NOT NULL,
    inbox_relays TEXT NOT NULL,
    key_package_relays TEXT NOT NULL,
    last_synced INTEGER NOT NULL
);

-- Step 2: Copy data from old table to new table, with defaults for new columns
INSERT INTO accounts_new (
    pubkey, settings,
    nip65_relays, inbox_relays, key_package_relays,
    last_synced
)
SELECT
    pubkey,
    settings,
    '[]',  -- default value for nip65_relays
    '[]',  -- default value for inbox_relays
    '[]',  -- default value for key_package_relays
    last_synced
FROM accounts;

-- Step 3: Drop the old table
DROP TABLE accounts;

-- Step 4: Rename the new table to the original name
ALTER TABLE accounts_new RENAME TO accounts;
