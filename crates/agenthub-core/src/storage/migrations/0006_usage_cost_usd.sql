-- Cost is stored in pricing-table units (USD). No FX conversion.
-- Existing rows were written as CNY via a fixed 7.2 rate; reverse that once.
ALTER TABLE usage_records RENAME COLUMN cost_cny TO cost_usd;
UPDATE usage_records
SET cost_usd = ROUND(cost_usd / 7.2, 4)
WHERE cost_usd IS NOT NULL;
