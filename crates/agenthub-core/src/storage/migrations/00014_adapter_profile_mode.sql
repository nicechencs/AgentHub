-- Product bucket for Adapter page tabs (API conversion vs OAuth proxy).
-- Orthogonal to route / source_kind. Existing writers are API Key conversions
-- (native_endpoint + local_bridge), so DEFAULT 'api' backfills current rows.
ALTER TABLE adapter_profiles ADD COLUMN mode TEXT NOT NULL DEFAULT 'api'
    CHECK (mode IN ('api', 'oauth'));

CREATE INDEX IF NOT EXISTS idx_adapter_profiles_mode
    ON adapter_profiles (mode, route, status);
