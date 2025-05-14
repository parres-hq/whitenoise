-- Accounts table with JSON fields for complex objects
CREATE TABLE accounts (
    pubkey TEXT PRIMARY KEY,
    metadata JSONB NOT NULL,  -- JSON Nostr Metadata
    settings JSONB NOT NULL,  -- JSON AccountSettings
    onboarding JSONB NOT NULL,  -- JSON AccountOnboarding
    last_used INTEGER NOT NULL,
    last_synced INTEGER NOT NULL,
    active BOOLEAN NOT NULL DEFAULT FALSE
);

-- Create an index for faster lookups
CREATE INDEX idx_accounts_active ON accounts(active);

-- Create a unique partial index that only allows one TRUE value
CREATE UNIQUE INDEX idx_accounts_single_active ON accounts(active) WHERE active = TRUE;

-- Create a trigger to ensure only one active account
CREATE TRIGGER ensure_single_active_account
   BEFORE UPDATE ON accounts
   WHEN NEW.active = TRUE
BEGIN
    UPDATE accounts SET active = FALSE WHERE active = TRUE AND pubkey != NEW.pubkey;
END;

-- Account-specific relays table
CREATE TABLE account_relays (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    url TEXT NOT NULL,
    relay_type TEXT NOT NULL,
    account_pubkey TEXT NOT NULL,
    FOREIGN KEY (account_pubkey) REFERENCES accounts(pubkey) ON DELETE CASCADE
);

CREATE INDEX idx_account_relays_account ON account_relays(account_pubkey, relay_type);
