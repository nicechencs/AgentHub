-- Credential-free projections between a persisted source and an agent adapter.
CREATE TABLE IF NOT EXISTS adapter_profiles (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    source_kind TEXT NOT NULL CHECK (source_kind IN ('account', 'provider')),
    source_id TEXT NOT NULL,
    target_agent_id TEXT NOT NULL,
    route TEXT NOT NULL CHECK (route IN ('config_sync', 'native_endpoint', 'local_bridge')),
    status TEXT NOT NULL CHECK (status IN ('applying', 'active', 'needs_attention')),
    rule_id TEXT NOT NULL,
    rule_version TEXT NOT NULL,
    generated_provider_id TEXT,
    last_error_code TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_adapter_profiles_source_target
    ON adapter_profiles (source_kind, source_id, target_agent_id);

-- Deliberately no foreign key: compensation may outlive a deleted provider.
CREATE INDEX IF NOT EXISTS idx_adapter_profiles_generated_provider
    ON adapter_profiles (generated_provider_id);
