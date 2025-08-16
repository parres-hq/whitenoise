-- Remove all automatic updated_at triggers to prevent recursion
-- The application code will handle updating timestamps manually

-- Drop all existing updated_at triggers
DROP TRIGGER IF EXISTS update_users_updated_at;
DROP TRIGGER IF EXISTS update_accounts_new_updated_at;
DROP TRIGGER IF EXISTS update_accounts_updated_at;
DROP TRIGGER IF EXISTS update_account_follows_updated_at;
DROP TRIGGER IF EXISTS update_relays_updated_at;
DROP TRIGGER IF EXISTS update_user_relays_updated_at;
DROP TRIGGER IF EXISTS update_app_settings_updated_at;
