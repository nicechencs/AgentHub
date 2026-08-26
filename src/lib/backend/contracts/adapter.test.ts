import { describe, expect, it } from 'vitest';
import { isAdapterErrorCodeRetryable } from './adapter';

/**
 * Keep in lockstep with `adapter_retryable_classification_covers_restore_and_retryable_prefix`
 * in `src-tauri/src/commands/adapter/tests.rs`.
 */
const RETRYABLE_CODES = [
  'retryable:adapter.port_in_use',
  'adapter.port_in_use',
  'adapter.bridge_start',
  'adapter.bridge_upstream_auth',
  'adapter.bridge_restore_source',
  'adapter.bridge_restore_port',
] as const;

const NOT_RETRYABLE_CODES = [
  'needs_attention',
  'adapter.bridge_rollback',
  'adapter.bridge_stop',
  'not_found',
] as const;

describe('isAdapterErrorCodeRetryable', () => {
  it('matches the desktop retryable classification', () => {
    for (const code of RETRYABLE_CODES) {
      expect(isAdapterErrorCodeRetryable(code), code).toBe(true);
    }
    for (const code of NOT_RETRYABLE_CODES) {
      expect(isAdapterErrorCodeRetryable(code), code).toBe(false);
    }
  });
});
