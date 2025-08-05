ALTER TABLE accounts
  ADD COLUMN nip65_relays TEXT NOT NULL DEFAULT '[]';
ALTER TABLE accounts
  ADD COLUMN inbox_relays TEXT NOT NULL DEFAULT '[]';
ALTER TABLE accounts
  ADD COLUMN key_package_relays TEXT NOT NULL DEFAULT '[]';

ALTER TABLE accounts DROP COLUMN onboarding;
