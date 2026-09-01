ALTER TABLE connection_trash ADD COLUMN home TEXT NOT NULL DEFAULT 'connections';

CREATE INDEX IF NOT EXISTS idx_connection_trash_home_deleted
    ON connection_trash (home, deleted_at DESC);
