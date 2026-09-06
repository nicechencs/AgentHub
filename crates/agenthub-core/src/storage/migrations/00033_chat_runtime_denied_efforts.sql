-- Per-model reasoning efforts rejected by Codex at turn/start (catalog may over-report).
CREATE TABLE IF NOT EXISTS chat_runtime_denied_efforts (
    model_id TEXT NOT NULL,
    effort TEXT NOT NULL,
    denied_at TEXT NOT NULL,
    PRIMARY KEY (model_id, effort)
);
