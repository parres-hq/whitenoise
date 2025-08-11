-- New accounts table
CREATE TABLE accounts_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    pubkey TEXT NOT NULL, -- Hex encoded nostr public key
    user_id INTEGER NOT NULL, -- Foreign key to the users table
    settings JSONB NOT NULL,
    last_synced_at DATETIME,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- New users table
CREATE TABLE users_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    pubkey TEXT NOT NULL,
    metadata JSONB,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- New account_followers table
CREATE TABLE account_followers (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id INTEGER NOT NULL,
    user_id INTEGER NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- New relays table
CREATE TABLE relays (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    url TEXT NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- New user_relays table
CREATE TABLE user_relays (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL,
    relay_id INTEGER NOT NULL,
    relay_type TEXT NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Triggers to automatically update updated_at field on row updates

-- Trigger for accounts_new table
CREATE TRIGGER update_accounts_new_updated_at
    AFTER UPDATE ON accounts_new
    FOR EACH ROW
BEGIN
    UPDATE accounts_new SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
END;

-- Trigger for users_new table
CREATE TRIGGER update_users_new_updated_at
    AFTER UPDATE ON users_new
    FOR EACH ROW
BEGIN
    UPDATE users_new SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
END;

-- Trigger for account_followers table
CREATE TRIGGER update_account_followers_updated_at
    AFTER UPDATE ON account_followers
    FOR EACH ROW
BEGIN
    UPDATE account_followers SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
END;

-- Trigger for relays table
CREATE TRIGGER update_relays_updated_at
    AFTER UPDATE ON relays
    FOR EACH ROW
BEGIN
    UPDATE relays SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
END;

-- Trigger for user_relays table
CREATE TRIGGER update_user_relays_updated_at
    AFTER UPDATE ON user_relays
    FOR EACH ROW
BEGIN
    UPDATE user_relays SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
END;

