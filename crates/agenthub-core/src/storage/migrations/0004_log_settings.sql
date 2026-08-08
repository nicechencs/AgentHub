-- Logging prefs (level already seeded in 0001; retention is new).
INSERT OR IGNORE INTO settings (key, value) VALUES ('log_retention_days', '14');
