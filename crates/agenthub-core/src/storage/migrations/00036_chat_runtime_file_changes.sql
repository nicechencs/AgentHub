-- Persist protocol-provided file-change snippets with the pending approval
-- so the card can show path + intended edit after snapshot / process restart.
ALTER TABLE chat_runtime_requests ADD COLUMN file_changes_json TEXT NOT NULL DEFAULT '[]';
