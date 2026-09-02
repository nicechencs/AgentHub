-- Per-request usage observed by the local gateway (bridge) runtime.
-- Deliberately separate from `usage_records`: log-file collection already
-- records the same spend from agent session files, and merging the two
-- would double count. This table adds the attribution the logs lack:
-- which route profile, which pool account, latency and time-to-first-token.
CREATE TABLE IF NOT EXISTS gateway_usage (
    request_id TEXT PRIMARY KEY,
    ts TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    surface TEXT NOT NULL,
    upstream_channel TEXT,
    ticket_id TEXT,
    account_source_kind TEXT,
    account_source_id TEXT,
    model TEXT,
    upstream_model TEXT,
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    cached_input_tokens INTEGER,
    reasoning_tokens INTEGER,
    status TEXT NOT NULL,
    status_code INTEGER,
    error_class TEXT,
    latency_ms INTEGER,
    ttft_ms INTEGER,
    attempts INTEGER,
    session_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_gateway_usage_ts ON gateway_usage(ts);
CREATE INDEX IF NOT EXISTS idx_gateway_usage_profile_ts ON gateway_usage(profile_id, ts);
