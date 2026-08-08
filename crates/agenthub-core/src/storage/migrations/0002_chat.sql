-- Chat conversations + messages (multi-agent side-by-side)

CREATE TABLE IF NOT EXISTS conversations (
    id               TEXT PRIMARY KEY,
    title            TEXT NOT NULL DEFAULT '',
    agent_ids        TEXT NOT NULL DEFAULT '[]',
    cwd              TEXT,
    allow_dangerous  INTEGER NOT NULL DEFAULT 0,
    created_at       TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at       TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS chat_messages (
    id               TEXT PRIMARY KEY,
    conversation_id  TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    turn             INTEGER NOT NULL,
    role             TEXT NOT NULL,
    agent_id         TEXT,
    content          TEXT NOT NULL DEFAULT '',
    status           TEXT NOT NULL DEFAULT 'ok',
    exit_code        INTEGER,
    duration_ms      INTEGER NOT NULL DEFAULT 0,
    error            TEXT,
    created_at       TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_chat_messages_conv
    ON chat_messages (conversation_id, turn, id);
