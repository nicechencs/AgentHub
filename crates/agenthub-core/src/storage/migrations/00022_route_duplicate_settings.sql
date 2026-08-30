-- Route create/import duplicate policy defaults.
INSERT OR IGNORE INTO settings (key, value) VALUES ('warn_duplicate_route_credential', 'true');
INSERT OR IGNORE INTO settings (key, value) VALUES ('update_duplicate_route_url', 'true');
