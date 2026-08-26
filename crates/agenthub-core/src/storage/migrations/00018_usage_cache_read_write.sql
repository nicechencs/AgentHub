-- Persist cache write vs read separately (billing rates differ).
-- Historical combined `cache_tokens` is copied onto cache_read_tokens until
-- the next usage_token_layout rebuild re-parses session logs.
ALTER TABLE usage_records ADD COLUMN cache_read_tokens INTEGER NOT NULL DEFAULT 0;
ALTER TABLE usage_records ADD COLUMN cache_write_tokens INTEGER NOT NULL DEFAULT 0;
UPDATE usage_records SET cache_read_tokens = COALESCE(cache_tokens, 0);
