CREATE TABLE accounts (
    pubkey TEXT PRIMARY KEY,
    settings JSONB NOT NULL,
    discovery_relays TEXT NOT NULL,
    inbox_relays TEXT NOT NULL,
    key_package_relays TEXT NOT NULL,
    last_synced INTEGER NOT NULL
);
