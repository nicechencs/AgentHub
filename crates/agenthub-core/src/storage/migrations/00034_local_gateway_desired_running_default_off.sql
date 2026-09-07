-- New installs leave this unset; the reader treats unset as off.
-- Existing stores that already used local forwarding keep the historical on default.
INSERT INTO settings (key, value)
SELECT 'local_gateway_desired_running', 'true'
WHERE NOT EXISTS (
    SELECT 1 FROM settings WHERE key = 'local_gateway_desired_running'
)
AND (
    EXISTS (
        SELECT 1 FROM adapter_profiles
        WHERE route = 'local_bridge'
          AND auto_start = 1
          AND status = 'active'
    )
    OR EXISTS (SELECT 1 FROM route_members)
    OR EXISTS (SELECT 1 FROM route_pools WHERE auto_start = 1)
    OR EXISTS (SELECT 1 FROM local_entry_keys)
);
