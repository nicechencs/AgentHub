/**
 * Named app limits aligned with `crates/agenthub-core/src/catalog/limits.rs`.
 * Keep values in lockstep; `app-limits.test.ts` asserts the Rust source.
 */

export const DEFAULT_LOG_RETENTION_DAYS = 14;
export const MIN_LOG_RETENTION_DAYS = 1;
export const MAX_LOG_RETENTION_DAYS = 365;
export const DEFAULT_USAGE_COLLECT_INTERVAL_MIN = 30;
export const MAX_USAGE_COLLECT_INTERVAL_MIN = 24 * 60;
