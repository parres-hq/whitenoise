-- Add unique index to user_relays table to prevent duplicate entries
CREATE UNIQUE INDEX IF NOT EXISTS idx_user_relays_unique
ON user_relays (user_id, relay_id, relay_type);

-- Step 1: Migrate contacts to users table
INSERT INTO users (pubkey, metadata)
SELECT
    pubkey,
    CASE
        WHEN json_valid(NULLIF(TRIM(metadata), ''))
             AND json_type(NULLIF(TRIM(metadata), '')) IN ('object')
        THEN NULLIF(TRIM(metadata), '')
        ELSE NULL
    END AS metadata
FROM contacts
WHERE NOT EXISTS (SELECT 1 FROM users WHERE users.pubkey = contacts.pubkey);

-- Step 2: Extract and insert unique relay URLs from contacts
-- Extract from nip65_relays
INSERT OR IGNORE INTO relays (url)
SELECT DISTINCT
    relay_value.value as url
FROM contacts,
     json_each(contacts.nip65_relays) as relay_value
WHERE json_valid(contacts.nip65_relays)
  AND relay_value.value IS NOT NULL
  AND relay_value.value != '';

-- Extract from inbox_relays
INSERT OR IGNORE INTO relays (url)
SELECT DISTINCT
    relay_value.value as url
FROM contacts,
     json_each(contacts.inbox_relays) as relay_value
WHERE json_valid(contacts.inbox_relays)
  AND relay_value.value IS NOT NULL
  AND relay_value.value != '';

-- Extract from key_package_relays
INSERT OR IGNORE INTO relays (url)
SELECT DISTINCT
    relay_value.value as url
FROM contacts,
     json_each(contacts.key_package_relays) as relay_value
WHERE json_valid(contacts.key_package_relays)
  AND relay_value.value IS NOT NULL
  AND relay_value.value != '';

-- Step 3: Create user_relays relationships
-- Insert nip65_relays relationships
INSERT OR IGNORE INTO user_relays (user_id, relay_id, relay_type)
SELECT DISTINCT
    u.id as user_id,
    r.id as relay_id,
    'nip65' as relay_type
FROM contacts c
JOIN users u ON u.pubkey = c.pubkey
CROSS JOIN json_each(c.nip65_relays) as relay_value
JOIN relays r ON r.url = relay_value.value
WHERE json_valid(c.nip65_relays)
  AND relay_value.value IS NOT NULL
  AND relay_value.value != '';

-- Insert inbox_relays relationships
INSERT OR IGNORE INTO user_relays (user_id, relay_id, relay_type)
SELECT DISTINCT
    u.id as user_id,
    r.id as relay_id,
    'inbox' as relay_type
FROM contacts c
JOIN users u ON u.pubkey = c.pubkey
CROSS JOIN json_each(c.inbox_relays) as relay_value
JOIN relays r ON r.url = relay_value.value
WHERE json_valid(c.inbox_relays)
  AND relay_value.value IS NOT NULL
  AND relay_value.value != '';

-- Insert key_package_relays relationships
INSERT OR IGNORE INTO user_relays (user_id, relay_id, relay_type)
SELECT DISTINCT
    u.id as user_id,
    r.id as relay_id,
    'key_package' as relay_type
FROM contacts c
JOIN users u ON u.pubkey = c.pubkey
CROSS JOIN json_each(c.key_package_relays) as relay_value
JOIN relays r ON r.url = relay_value.value
WHERE json_valid(c.key_package_relays)
  AND relay_value.value IS NOT NULL
  AND relay_value.value != '';

-- Step 4: Extract and insert unique relay URLs from accounts table
-- Extract from accounts.nip65_relays
INSERT OR IGNORE INTO relays (url)
SELECT DISTINCT
    relay_value.value as url
FROM accounts,
     json_each(accounts.nip65_relays) as relay_value
WHERE json_valid(accounts.nip65_relays)
  AND relay_value.value IS NOT NULL
  AND relay_value.value != '';

-- Extract from accounts.inbox_relays
INSERT OR IGNORE INTO relays (url)
SELECT DISTINCT
    relay_value.value as url
FROM accounts,
     json_each(accounts.inbox_relays) as relay_value
WHERE json_valid(accounts.inbox_relays)
  AND relay_value.value IS NOT NULL
  AND relay_value.value != '';

-- Extract from accounts.key_package_relays
INSERT OR IGNORE INTO relays (url)
SELECT DISTINCT
    relay_value.value as url
FROM accounts,
     json_each(accounts.key_package_relays) as relay_value
WHERE json_valid(accounts.key_package_relays)
  AND relay_value.value IS NOT NULL
  AND relay_value.value != '';

-- Step 5: Create user_relays relationships from accounts table
-- Insert accounts.nip65_relays relationships
INSERT OR IGNORE INTO user_relays (user_id, relay_id, relay_type)
SELECT DISTINCT
    u.id as user_id,
    r.id as relay_id,
    'nip65' as relay_type
FROM accounts a
JOIN users u ON u.pubkey = a.pubkey
CROSS JOIN json_each(a.nip65_relays) as relay_value
JOIN relays r ON r.url = relay_value.value
WHERE json_valid(a.nip65_relays)
  AND relay_value.value IS NOT NULL
  AND relay_value.value != '';

-- Insert accounts.inbox_relays relationships
INSERT OR IGNORE INTO user_relays (user_id, relay_id, relay_type)
SELECT DISTINCT
    u.id as user_id,
    r.id as relay_id,
    'inbox' as relay_type
FROM accounts a
JOIN users u ON u.pubkey = a.pubkey
CROSS JOIN json_each(a.inbox_relays) as relay_value
JOIN relays r ON r.url = relay_value.value
WHERE json_valid(a.inbox_relays)
  AND relay_value.value IS NOT NULL
  AND relay_value.value != '';

-- Insert accounts.key_package_relays relationships
INSERT OR IGNORE INTO user_relays (user_id, relay_id, relay_type)
SELECT DISTINCT
    u.id as user_id,
    r.id as relay_id,
    'key_package' as relay_type
FROM accounts a
JOIN users u ON u.pubkey = a.pubkey
CROSS JOIN json_each(a.key_package_relays) as relay_value
JOIN relays r ON r.url = relay_value.value
WHERE json_valid(a.key_package_relays)
  AND relay_value.value IS NOT NULL
  AND relay_value.value != '';

-- Step 6: Migrate accounts to accounts_new table
INSERT INTO accounts_new (pubkey, user_id, settings, last_synced_at)
SELECT
    a.pubkey,
    u.id as user_id,
    a.settings,
    CASE
        WHEN a.last_synced IS NOT NULL THEN datetime(a.last_synced, 'unixepoch')
        ELSE NULL
    END as last_synced_at
FROM accounts a
JOIN users u ON u.pubkey = a.pubkey
WHERE NOT EXISTS (SELECT 1 FROM accounts_new WHERE accounts_new.pubkey = a.pubkey);
