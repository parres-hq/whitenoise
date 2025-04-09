-- Add uniqueness constraint to account_relays table to prevent duplicate url + relay_type for the same account
CREATE UNIQUE INDEX idx_account_relays_unique ON account_relays(account_pubkey, url, relay_type);

-- Add uniqueness constraint to group_relays table to prevent duplicate url + relay_type for the same group and account
CREATE UNIQUE INDEX idx_group_relays_unique ON group_relays(group_id, account_pubkey, url, relay_type);
