CREATE TABLE IF NOT EXISTS connection_trash (
    id           TEXT PRIMARY KEY,
    agent_id     TEXT NOT NULL,
    source_kind  TEXT NOT NULL,
    source_id    TEXT NOT NULL,
    label        TEXT NOT NULL,
    was_current  INTEGER NOT NULL DEFAULT 0,
    payload      TEXT NOT NULL,
    deleted_at   TEXT NOT NULL,
    expires_at   TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_connection_trash_agent_deleted
    ON connection_trash (agent_id, deleted_at DESC);
