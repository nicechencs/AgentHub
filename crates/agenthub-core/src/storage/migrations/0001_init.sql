-- P0 initial schema (tables for later phases; P0 only needs settings + migrations)

CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS providers (
    id               TEXT PRIMARY KEY,
    agent_id         TEXT NOT NULL,
    name             TEXT NOT NULL,
    settings_config  TEXT NOT NULL DEFAULT '{}',
    meta             TEXT NOT NULL DEFAULT '{}',
    is_current       INTEGER NOT NULL DEFAULT 0,
    created_at       TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at       TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS accounts (
    id           TEXT PRIMARY KEY,
    agent_id     TEXT NOT NULL,
    kind         TEXT NOT NULL,
    label        TEXT,
    credentials  TEXT NOT NULL DEFAULT '',
    extra        TEXT NOT NULL DEFAULT '{}',
    status       TEXT NOT NULL DEFAULT 'active',
    is_current   INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS skills (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    source     TEXT,
    meta       TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS usage_records (
    id          TEXT PRIMARY KEY,
    agent_id    TEXT NOT NULL,
    account_id  TEXT,
    model       TEXT,
    input_tokens  INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    cache_tokens  INTEGER NOT NULL DEFAULT 0,
    cost_cny    REAL,
    session_id  TEXT,
    ts          TEXT NOT NULL,
    raw_hash    TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_usage_dedup
    ON usage_records (agent_id, session_id, raw_hash);

CREATE TABLE IF NOT EXISTS backups (
    id          TEXT PRIMARY KEY,
    agent_id    TEXT,
    kind        TEXT NOT NULL,
    path        TEXT NOT NULL,
    files       TEXT NOT NULL DEFAULT '[]',
    size        INTEGER NOT NULL DEFAULT 0,
    note        TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT OR IGNORE INTO settings (key, value) VALUES ('theme', 'system');
INSERT OR IGNORE INTO settings (key, value) VALUES ('language', 'zh-CN');
INSERT OR IGNORE INTO settings (key, value) VALUES ('log_level', 'info');
