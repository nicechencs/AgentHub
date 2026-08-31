-- Fingerprint of the exact bytes AgentHub last wrote into each live agent
-- config file. Used to detect hand edits made outside AgentHub before the
-- file is rewritten or restored, and to prove a created file is still ours
-- before it is removed again on restore.
CREATE TABLE IF NOT EXISTS live_write_fingerprints (
    agent_id TEXT NOT NULL,
    path TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    written_at TEXT NOT NULL,
    PRIMARY KEY (agent_id, path)
);
