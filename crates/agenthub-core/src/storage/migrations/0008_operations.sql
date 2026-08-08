-- Lifecycle operation audit / recovery (append-only style rows; not install fact source).
CREATE TABLE IF NOT EXISTS operations (
    id TEXT PRIMARY KEY NOT NULL,
    agent_key TEXT NOT NULL,
    kind TEXT NOT NULL,
    status TEXT NOT NULL,
    step TEXT,
    error_code TEXT,
    summary TEXT,
    observed_status TEXT,
    observed_version TEXT,
    started_at TEXT NOT NULL,
    finished_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_operations_agent_started
    ON operations (agent_key, started_at DESC);

CREATE INDEX IF NOT EXISTS idx_operations_status
    ON operations (status);
