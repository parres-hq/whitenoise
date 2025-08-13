CREATE TABLE IF NOT EXISTS app_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1), -- Only one row allowed with id=1
    theme_mode TEXT NOT NULL DEFAULT 'system',
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Insert default app settings row with system theme
INSERT INTO app_settings (theme_mode) VALUES ('system');

-- Trigger to automatically update updated_at field on row updates
CREATE TRIGGER update_app_settings_updated_at
    AFTER UPDATE ON app_settings
    FOR EACH ROW
BEGIN
    UPDATE app_settings SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
END;


DROP TABLE IF EXISTS accounts;
DROP TABLE IF EXISTS contacts;

RENAME TABLE accounts_new TO accounts;

ALTER TABLE accounts DROP COLUMN settings;

