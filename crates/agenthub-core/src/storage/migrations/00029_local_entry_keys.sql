-- Named extra loopback bearers for the tokens page.
-- Empty token = display name for the pool's default hub_token (not a second bearer).
CREATE TABLE local_entry_keys (
    id TEXT PRIMARY KEY,
    pool_id TEXT NOT NULL,
    name TEXT NOT NULL,
    token TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX local_entry_keys_token
    ON local_entry_keys(token)
    WHERE token != '';

CREATE INDEX local_entry_keys_pool
    ON local_entry_keys(pool_id);
