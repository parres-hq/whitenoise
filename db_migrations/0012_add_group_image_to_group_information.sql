-- Migration: Add group_image column to group_information

ALTER TABLE group_information
ADD COLUMN group_image BLOB;
