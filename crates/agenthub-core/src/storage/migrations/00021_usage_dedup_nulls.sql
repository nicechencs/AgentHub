-- UNIQUE treats NULLs as distinct, so rescans with missing session_id/raw_hash
-- duplicated rows and polluted usage totals. Normalize then re-index with
-- coalesce so the conflict key is stable even if a future writer inserts NULL.

UPDATE usage_records SET session_id = '' WHERE session_id IS NULL;
UPDATE usage_records SET raw_hash = '' WHERE raw_hash IS NULL;

-- Rows missing both identifiers would otherwise share (agent_id, '', '') and
-- collapse to one per agent. Stamp a stable unique sentinel first.
UPDATE usage_records SET raw_hash = 'orphan:' || id WHERE session_id = '' AND raw_hash = '';

DELETE FROM usage_records
WHERE rowid NOT IN (
    SELECT MAX(rowid)
    FROM usage_records
    GROUP BY agent_id, session_id, raw_hash
);

DROP INDEX IF EXISTS idx_usage_dedup;
CREATE UNIQUE INDEX IF NOT EXISTS idx_usage_dedup
    ON usage_records (agent_id, ifnull(session_id, ''), ifnull(raw_hash, ''));
