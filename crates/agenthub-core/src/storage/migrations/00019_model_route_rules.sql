-- Explicit public_model → upstream lane rules for mixed-provider pools.
-- Operators insert rows; member listed models do not auto-create rules.
-- equivalent_group is NULL unless lanes are declared equivalent for failover.

CREATE TABLE IF NOT EXISTS model_route_rules (
    id TEXT PRIMARY KEY NOT NULL,
    route_pool_id TEXT NOT NULL REFERENCES route_pools(id) ON DELETE CASCADE,
    public_model TEXT NOT NULL,
    endpoint_family TEXT NOT NULL,
    upstream_provider TEXT NOT NULL,
    upstream_dialect TEXT NOT NULL,
    upstream_model TEXT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 0,
    equivalent_group TEXT,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_model_route_rules_lane
    ON model_route_rules (
        route_pool_id,
        public_model,
        endpoint_family,
        upstream_provider,
        upstream_model
    );

CREATE INDEX IF NOT EXISTS idx_model_route_rules_lookup
    ON model_route_rules (
        route_pool_id,
        endpoint_family,
        public_model,
        priority ASC,
        id ASC
    );
