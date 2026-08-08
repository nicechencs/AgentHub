CREATE TABLE IF NOT EXISTS usage_parser_health (
    agent_id TEXT PRIMARY KEY,
    supported INTEGER NOT NULL,
    records INTEGER NOT NULL DEFAULT 0,
    fail_rate_pct REAL,
    skipped INTEGER,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

