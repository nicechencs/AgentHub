-- Codex Fast/Priority must survive collect-time cost recompute.
ALTER TABLE usage_records ADD COLUMN fast INTEGER NOT NULL DEFAULT 0;
