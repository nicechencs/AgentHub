-- Unique current pointer per agent (P10). Old accounts/providers.is_current remain as dual-write.
CREATE TABLE IF NOT EXISTS agent_active_bindings (
    agent_key TEXT PRIMARY KEY NOT NULL,
    account_id TEXT,
    provider_id TEXT,
    model_id TEXT,
    config_profile_id TEXT,
    revision INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_agent_active_bindings_account
    ON agent_active_bindings (account_id);

CREATE INDEX IF NOT EXISTS idx_agent_active_bindings_provider
    ON agent_active_bindings (provider_id);
