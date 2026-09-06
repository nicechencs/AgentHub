import { describe, expect, it } from 'vitest';
import {
  CHAT_ACTIONS,
  actionMatchesQuery,
  chatActionDisabledReason,
  clampActionIndex,
  filterChatActions,
  isCommandSearchMode,
  normalizeActionQuery,
} from './chat-actions';

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

  it('filters with Chinese-friendly normalization', () => {
    expect(filterChatActions('/').length).toBe(CHAT_ACTIONS.length);
    expect(filterChatActions('/新建').some((item) => item.id === 'new-session')).toBe(true);
    expect(filterChatActions('/搜索').some((item) => item.id === 'focus-history-search')).toBe(true);
    expect(filterChatActions('/copy').some((item) => item.id === 'copy-latest-reply')).toBe(true);
    expect(filterChatActions('/报错').some((item) => item.id === 'sample-explain-error')).toBe(true);
    expect(normalizeActionQuery('  新建  ')).toBe('新建');
    expect(actionMatchesQuery(CHAT_ACTIONS[0], 'new')).toBe(true);
  });

  it('exposes disabled reasons without wrapping as prompts', () => {
    expect(
      chatActionDisabledReason(
        CHAT_ACTIONS.find((item) => item.id === 'copy-latest-reply')!,
        { hasLatestReply: false, newChatAllowed: true },
      ),
    ).toBe('noReply');
    expect(
      chatActionDisabledReason(
        CHAT_ACTIONS.find((item) => item.id === 'new-session')!,
        { hasLatestReply: true, newChatAllowed: false },
      ),
    ).toBe('noAgent');
    expect(clampActionIndex(-1, 3)).toBe(2);
    expect(clampActionIndex(3, 3)).toBe(0);
  });
});
