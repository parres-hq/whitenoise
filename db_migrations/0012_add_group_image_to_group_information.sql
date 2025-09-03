-- Migration: Store local file pointer for the group image (no inline BLOB)

ALTER TABLE group_information
  ADD COLUMN image_pointer TEXT;
