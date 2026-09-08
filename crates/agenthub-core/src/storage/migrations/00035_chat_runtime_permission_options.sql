-- Persist ACP permission option ids/kinds with the pending request so
-- Allow Always survives snapshot and process restart.
ALTER TABLE chat_runtime_requests ADD COLUMN options_json TEXT NOT NULL DEFAULT '[]';
