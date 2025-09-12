-- Normalize relay URLs to ensure consistent storage
-- This migration removes ALL trailing slashes from URLs to ensure consistency
-- Since most WebSocket/relay implementations treat URLs with and without trailing
-- slashes as equivalent, normalizing to no trailing slash provides the cleanest
-- and most predictable behavior

-- Remove trailing slashes from all URLs
UPDATE relays
SET url = RTRIM(url, '/')
WHERE url LIKE '%/';

-- Handle any potential duplicates that might have been created
-- If after normalization we have duplicate URLs, merge them by keeping the older record
-- and updating any references to point to the older record

-- First, identify duplicate URLs (this query will show them if any exist)
-- We'll use a more complex approach to handle duplicates properly

-- Create a temporary table to store URL mappings
CREATE TEMPORARY TABLE relay_url_mapping AS
SELECT
    MIN(id) as keep_id,
    MAX(id) as remove_id,
    url
FROM relays
GROUP BY url
HAVING COUNT(*) > 1;

-- Update user_relays to point to the records we're keeping
UPDATE user_relays
SET relay_id = (
    SELECT keep_id
    FROM relay_url_mapping
    WHERE relay_url_mapping.remove_id = user_relays.relay_id
)
WHERE relay_id IN (SELECT remove_id FROM relay_url_mapping);

-- Remove the duplicate relay records
DELETE FROM relays
WHERE id IN (SELECT remove_id FROM relay_url_mapping);

-- Drop the temporary table
DROP TABLE relay_url_mapping;
