-- P12: shared skill packages + per-agent desired assignments (reconcile targets).
CREATE TABLE IF NOT EXISTS skill_packages (
    id TEXT PRIMARY KEY NOT NULL,
    source_kind TEXT NOT NULL,
    locator TEXT NOT NULL,
    revision TEXT NOT NULL,
    manifest_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS skill_assignments (
    skill_package_id TEXT NOT NULL,
    agent_key TEXT NOT NULL,
    desired_enabled INTEGER NOT NULL DEFAULT 0,
    projection_mode TEXT NOT NULL DEFAULT 'copy',
    applied_revision TEXT,
    observed_status TEXT NOT NULL DEFAULT 'pending',
    last_error TEXT,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (skill_package_id, agent_key),
    FOREIGN KEY (skill_package_id) REFERENCES skill_packages(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_skill_assignments_agent_key
    ON skill_assignments (agent_key);

CREATE INDEX IF NOT EXISTS idx_skill_assignments_observed_status
    ON skill_assignments (observed_status);
