-- Domain name is unified_gateway_enrolled; drop the historical column label.
ALTER TABLE route_pools RENAME COLUMN v2_enrolled TO unified_gateway_enrolled;
