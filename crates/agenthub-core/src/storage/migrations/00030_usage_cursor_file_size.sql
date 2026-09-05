-- Collect treats a file as modified when size *or* mtime changes.
-- CREATE is for product DBs that dropped usage_cursors after cache isolation.
CREATE TABLE IF NOT EXISTS usage_cursors (
    path       TEXT PRIMARY KEY,
    agent_id   TEXT NOT NULL,
    byte_offset INTEGER NOT NULL DEFAULT 0,
    file_mtime  INTEGER NOT NULL DEFAULT 0,
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

ALTER TABLE usage_cursors ADD COLUMN file_size INTEGER NOT NULL DEFAULT 0;
