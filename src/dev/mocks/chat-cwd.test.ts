import { describe, expect, it } from 'vitest';
import { mockCwdMissing } from './chat';

describe('mockCwdMissing', () => {
  it('treats temp and missing-cwd sentinels as gone', () => {
    expect(mockCwdMissing('/var/folders/zz/T/.tmp-agenthub-missing/workspace')).toBe(true);
    expect(mockCwdMissing('C:\\Users\\demo\\app')).toBe(false);
    expect(mockCwdMissing(null)).toBe(false);
    expect(mockCwdMissing('')).toBe(false);
  });
});
