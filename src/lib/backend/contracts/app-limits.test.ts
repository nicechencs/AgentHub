import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import {
  DEFAULT_LOG_RETENTION_DAYS,
  DEFAULT_USAGE_COLLECT_INTERVAL_MIN,
  MAX_LOG_RETENTION_DAYS,
  MAX_USAGE_COLLECT_INTERVAL_MIN,
  MIN_LOG_RETENTION_DAYS,
} from './app-limits';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../../..');

describe('app limits catalog mirror', () => {
  it('matches crates/agenthub-core/src/catalog/limits.rs', () => {
    const limitsRs = readFileSync(
      path.join(root, 'crates/agenthub-core/src/catalog/limits.rs'),
      'utf8',
    );
    expect(limitsRs).toContain('pub const DEFAULT_LOG_RETENTION_DAYS: u32 = 14;');
    expect(limitsRs).toContain('pub const DEFAULT_USAGE_COLLECT_INTERVAL_MIN: u32 = 30;');
    expect(limitsRs).toContain('pub const MAX_USAGE_COLLECT_INTERVAL_MIN: u32 = 24 * 60;');
    expect(limitsRs).toContain('pub const MIN_LOG_RETENTION_DAYS: u32 = 1;');
    expect(limitsRs).toContain('pub const MAX_LOG_RETENTION_DAYS: u32 = 365;');

    expect(DEFAULT_LOG_RETENTION_DAYS).toBe(14);
    expect(DEFAULT_USAGE_COLLECT_INTERVAL_MIN).toBe(30);
    expect(MAX_USAGE_COLLECT_INTERVAL_MIN).toBe(24 * 60);
    expect(MIN_LOG_RETENTION_DAYS).toBe(1);
    expect(MAX_LOG_RETENTION_DAYS).toBe(365);
  });
});
