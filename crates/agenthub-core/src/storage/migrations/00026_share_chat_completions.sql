-- User choice: Kimi and DSH may share one chat-completions local token.
-- Default off keeps today's per-Agent pools.
INSERT OR IGNORE INTO settings (key, value) VALUES ('share_chat_completions', 'false');
