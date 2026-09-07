import { describe, expect, it } from 'vitest';
import { folderNameFromCwd } from './open-chat-cwd';

describe('folderNameFromCwd', () => {
  it('uses the last folder name on Windows and POSIX paths', () => {
    expect(folderNameFromCwd('D:\\work\\AgentHub')).toBe('AgentHub');
    expect(folderNameFromCwd('D:\\work\\AgentHub\\')).toBe('AgentHub');
    expect(folderNameFromCwd('/Users/demo/src')).toBe('src');
  });

  it('returns empty for blank input', () => {
    expect(folderNameFromCwd('   ')).toBe('');
  });
});
