-- Next-turn model/effort for Codex chat runtime (B2).
ALTER TABLE chat_runtime ADD COLUMN next_model TEXT;
ALTER TABLE chat_runtime ADD COLUMN next_effort TEXT;
