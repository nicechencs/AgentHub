-- Incremental parse cursors for UsageParser (file offset / mtime).

CREATE TABLE IF NOT EXISTS usage_cursors (
    path       TEXT PRIMARY KEY,
    agent_id   TEXT NOT NULL,
    byte_offset INTEGER NOT NULL DEFAULT 0,
    file_mtime  INTEGER NOT NULL DEFAULT 0,
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_usage_records_ts ON usage_records (ts);
CREATE INDEX IF NOT EXISTS idx_usage_records_agent_ts ON usage_records (agent_id, ts);
