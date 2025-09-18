-- Preserve accounts at all costs
-- Strategy: Create users directly from accounts, ignore contacts entirely
-- This ensures 1:1 mapping between old accounts and new accounts

PRAGMA foreign_keys = ON;

-- Add unique index to user_relays table to prevent duplicate entries
CREATE UNIQUE INDEX IF NOT EXISTS idx_user_relays_unique
ON user_relays (user_id, relay_id, relay_type);

-- STEP 1: Create users table entries DIRECTLY from accounts table
-- This ensures every account will have a corresponding user
-- We ignore contacts entirely to avoid any filtering/joining issues
INSERT INTO users (pubkey, metadata)
SELECT DISTINCT
    a.pubkey,
    '{}' as metadata  -- Empty JSON object
FROM accounts a
WHERE a.pubkey IS NOT NULL
  AND TRIM(a.pubkey) != '';

-- STEP 2: Migrate ALL accounts to accounts_new table
-- Since we created users directly from accounts, this JOIN should never fail
INSERT INTO accounts_new (pubkey, user_id, settings, last_synced_at)
SELECT
    a.pubkey,
    u.id as user_id,
    a.settings,
    NULL as last_synced_at
FROM accounts a
JOIN users u ON u.pubkey = a.pubkey
WHERE a.pubkey IS NOT NULL
  AND TRIM(a.pubkey) != '';
