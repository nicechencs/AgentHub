-- Local-bridge runtime metadata. Source credentials remain exclusively in
-- their connection/provider row; generated local bearer tokens are stored in
-- the generated Codex provider auth payload, never in this profile table.
ALTER TABLE adapter_profiles ADD COLUMN local_port INTEGER;
ALTER TABLE adapter_profiles ADD COLUMN auto_start INTEGER NOT NULL DEFAULT 0
    CHECK (auto_start IN (0, 1));

CREATE INDEX IF NOT EXISTS idx_adapter_profiles_bridge_restore
    ON adapter_profiles (route, auto_start, status);
