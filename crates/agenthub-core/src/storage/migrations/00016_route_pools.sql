-- Normalized RoutePool / RouteMember relations for the unified loopback pool.
-- Hub tokens are local loopback bearers, not upstream credentials.
-- Members store authorization references only (source_kind + source_id).

CREATE TABLE IF NOT EXISTS route_pools (
    id TEXT PRIMARY KEY NOT NULL,
    target_agent_id TEXT NOT NULL,
    downstream_surface TEXT NOT NULL
        CHECK (downstream_surface IN ('responses', 'messages', 'chat_completions')),
    downstream_dialect TEXT NOT NULL
        CHECK (downstream_dialect IN ('claude', 'codex', 'grok', 'kimi', 'dsh', 'generic')),
    hub_token TEXT NOT NULL,
    schedule_policy TEXT NOT NULL DEFAULT 'priority_failover'
        CHECK (schedule_policy IN ('priority_failover', 'round_robin')),
    is_default INTEGER NOT NULL DEFAULT 0 CHECK (is_default IN (0, 1)),
    v2_enrolled INTEGER NOT NULL DEFAULT 0 CHECK (v2_enrolled IN (0, 1)),
    policy_revision INTEGER NOT NULL DEFAULT 1,
    auto_start INTEGER NOT NULL DEFAULT 0 CHECK (auto_start IN (0, 1)),
    gateway_port INTEGER,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_route_pools_hub_token
    ON route_pools (hub_token);

CREATE UNIQUE INDEX IF NOT EXISTS idx_route_pools_default
    ON route_pools (target_agent_id, downstream_surface)
    WHERE is_default = 1;

CREATE INDEX IF NOT EXISTS idx_route_pools_agent_surface
    ON route_pools (target_agent_id, downstream_surface, id);

CREATE TABLE IF NOT EXISTS route_members (
    id TEXT PRIMARY KEY NOT NULL,
    route_pool_id TEXT NOT NULL REFERENCES route_pools(id) ON DELETE CASCADE,
    source_kind TEXT NOT NULL CHECK (source_kind IN ('account', 'provider')),
    source_id TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    priority INTEGER NOT NULL DEFAULT 0,
    position INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_route_members_auth
    ON route_members (route_pool_id, source_kind, source_id);

CREATE INDEX IF NOT EXISTS idx_route_members_pool_order
    ON route_members (route_pool_id, priority ASC, position ASC, id ASC);
