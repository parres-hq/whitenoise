-- Add unique constraint to account_follows table to prevent duplicate follows
-- This constraint ensures that an account can only follow a user once
CREATE UNIQUE INDEX IF NOT EXISTS idx_account_follows_unique
ON account_follows(account_id, user_id);
