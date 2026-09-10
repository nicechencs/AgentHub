import { afterEach, describe, expect, it } from 'vitest';
import { forgetFallbackCwd, peekFallbackCwd, rememberFallbackCwd } from './chat-cwd-fallback';

describe('chat-cwd-fallback', () => {
  afterEach(() => {
    forgetFallbackCwd('conv-1');
  });

  it('remembers a live project folder for rebind', () => {
    rememberFallbackCwd('conv-1', '  D:\\work\\app  ');
    expect(peekFallbackCwd('conv-1')).toBe('D:\\work\\app');
    forgetFallbackCwd('conv-1');
    expect(peekFallbackCwd('conv-1')).toBeNull();
  });
});
