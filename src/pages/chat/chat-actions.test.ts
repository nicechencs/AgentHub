import { describe, expect, it } from 'vitest';
import { filterChatActions, isCommandSearchMode } from './chat-actions';

describe('chat action command search', () => {
  it('opens only for explicit / command tokens', () => {
    expect(isCommandSearchMode('/')).toBe(true);
    expect(isCommandSearchMode('/new')).toBe(true);
    expect(isCommandSearchMode('')).toBe(false);
    expect(isCommandSearchMode('path/to/file')).toBe(false);
    expect(isCommandSearchMode('use /tmp')).toBe(false);
    expect(isCommandSearchMode('https://example.com/a')).toBe(false);
    expect(isCommandSearchMode('code `/foo`')).toBe(false);
  });

  it('filters the shared action list', () => {
    expect(filterChatActions('/').length).toBeGreaterThan(3);
    expect(filterChatActions('/新建').some((item) => item.id === 'new-session')).toBe(true);
    expect(filterChatActions('/copy').some((item) => item.id === 'copy-latest-reply')).toBe(true);
  });
});
