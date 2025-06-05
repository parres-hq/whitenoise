-- Accounts table with JSON fields for complex objects
CREATE TABLE accounts (
    pubkey TEXT PRIMARY KEY,
    settings JSONB NOT NULL,  -- JSON AccountSettings
    onboarding JSONB NOT NULL,  -- JSON AccountOnboarding
    nwc JSONB NOT NULL,        -- JSON AccountNwc
    last_synced INTEGER NOT NULL
);
