-- Accounts table with JSON fields for complex objects
CREATE TABLE accounts (
    pubkey TEXT PRIMARY KEY,
    metadata JSONB NOT NULL,  -- JSON Nostr Metadata
    settings JSONB NOT NULL,  -- JSON AccountSettings
    onboarding JSONB NOT NULL,  -- JSON AccountOnboarding
    relays JSONB NOT NULL,     -- JSON AccountRelays
    nwc JSONB NOT NULL,        -- JSON AccountNwc
    last_used INTEGER NOT NULL,
    last_synced INTEGER NOT NULL
);
