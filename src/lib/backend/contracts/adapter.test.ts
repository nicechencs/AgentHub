import { describe, expect, it } from 'vitest';
import { isAdapterErrorCodeRetryable } from './adapter';
import contract from './retryable-error-contract.json';

describe('isAdapterErrorCodeRetryable', () => {
  it('matches the shared retryable error contract', () => {
    for (const code of contract.retryableExact) {
      expect(isAdapterErrorCodeRetryable(code), code).toBe(true);
    }
    for (const example of contract.retryablePrefixExamples) {
      expect(
        contract.retryablePrefixes.some((prefix) => example.startsWith(prefix)),
        example,
      ).toBe(true);
      expect(isAdapterErrorCodeRetryable(example), example).toBe(true);
    }
    for (const prefix of contract.retryablePrefixes) {
      expect(
        contract.retryablePrefixExamples.some((example) => example.startsWith(prefix)),
        prefix,
      ).toBe(true);
      expect(isAdapterErrorCodeRetryable(`${prefix}x`), `${prefix}x`).toBe(true);
    }
    for (const code of contract.notRetryableExamples) {
      expect(isAdapterErrorCodeRetryable(code), code).toBe(false);
    }
  });
});
