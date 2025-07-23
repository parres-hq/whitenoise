CREATE TABLE contacts (
    pubkey TEXT PRIMARY KEY NOT NULL,
    metadata TEXT,
    discovery_relays TEXT NOT NULL,
    inbox_relays TEXT NOT NULL,
    key_package_relays TEXT NOT NULL
);
