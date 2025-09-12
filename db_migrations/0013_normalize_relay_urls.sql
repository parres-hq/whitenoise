-- Normalize relay URLs to ensure consistent storage
-- This migration removes ALL trailing slashes from URLs to ensure consistency
-- Since most WebSocket/relay implementations treat URLs with and without trailing
-- slashes as equivalent, normalizing to no trailing slash provides the cleanest
-- and most predictable behavior
--
-- This migration is idempotent and safe to run multiple times.
-- It uses a multi-step approach to avoid UNIQUE constraint violations:
-- 1. Compute normalized URLs in a CTE
-- 2. Identify keeper rows (oldest/smallest ID per normalized URL)
-- 3. Update foreign key references to point to keepers
-- 4. Delete duplicate rows
-- 5. Update keeper rows to normalized URLs

-- Step 1: Create a temporary table with normalized URL mappings
-- This identifies which relays will be kept and which will be removed
-- Handles ALL duplicates correctly (3+ duplicates) by keeping MIN(id) per normalized URL
CREATE TEMPORARY TABLE relay_normalization_plan AS
WITH normalized_relays AS (
    -- Compute normalized URL for every relay
    SELECT
        id,
        url,
        RTRIM(url, '/') as normalized_url
    FROM relays
),
keeper_selection AS (
    -- For each normalized URL, select the keeper (oldest/smallest ID)
    SELECT
        normalized_url,
        MIN(id) as keeper_id
    FROM normalized_relays
    GROUP BY normalized_url
)
SELECT
    nr.id as original_id,
    nr.url as original_url,
    nr.normalized_url,
    ks.keeper_id
FROM normalized_relays nr
JOIN keeper_selection ks ON nr.normalized_url = ks.normalized_url;

-- Step 2: Update foreign key references in user_relays
-- Point all relay_id references to the keeper for that normalized URL
-- This handles ALL non-keeper IDs (not just MAX, but any ID != MIN)
UPDATE user_relays
SET relay_id = (
    SELECT keeper_id
    FROM relay_normalization_plan
    WHERE original_id = user_relays.relay_id
)
WHERE relay_id IN (
    SELECT original_id
    FROM relay_normalization_plan
    WHERE original_id != keeper_id
);

-- Step 3: Delete ALL duplicate relay rows (non-keepers)
-- This removes every relay except the keeper (MIN(id)) for each normalized URL
DELETE FROM relays
WHERE id IN (
    SELECT original_id
    FROM relay_normalization_plan
    WHERE original_id != keeper_id
);

-- Step 4: Update keeper rows to have normalized URLs
-- Only update if the URL actually needs normalization (idempotent)
UPDATE relays
SET url = (
    SELECT normalized_url
    FROM relay_normalization_plan
    WHERE original_id = relays.id
)
WHERE id IN (
    SELECT original_id
    FROM relay_normalization_plan
    WHERE original_id = keeper_id AND original_url != normalized_url
);

-- Clean up temporary table
DROP TABLE relay_normalization_plan;
