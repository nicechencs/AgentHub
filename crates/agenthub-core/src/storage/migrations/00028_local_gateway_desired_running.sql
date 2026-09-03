-- Shared local-gateway switch; drop the historical settings key label.
UPDATE settings SET key = 'local_gateway_desired_running' WHERE key = 'local_entry_desired_running';
