import { describe, expect, it } from 'vitest';
import {
  CHAT_ACTIONS,
  actionMatchesQuery,
  chatActionDisabledReason,
  chatOverflowMenuActions,
  chatStarterActions,
  clampActionIndex,
  filterChatActions,
  isCommandSearchMode,
  normalizeActionQuery,
  slashMenuFixedPosition,
} from './chat-actions';

describe('chat action command search', () => {
  it('opens only for explicit / or \\ command tokens', () => {
    expect(isCommandSearchMode('/')).toBe(true);
    expect(isCommandSearchMode('/new')).toBe(true);
    expect(isCommandSearchMode('\\')).toBe(true);
    expect(isCommandSearchMode('\\model')).toBe(true);
    expect(isCommandSearchMode('')).toBe(false);
    expect(isCommandSearchMode('path/to/file')).toBe(false);
    expect(isCommandSearchMode('use /tmp')).toBe(false);
    expect(isCommandSearchMode('https://example.com/a')).toBe(false);
    expect(isCommandSearchMode('code `/foo`')).toBe(false);
  });

  it('filters with Chinese-friendly normalization', () => {
    expect(filterChatActions('/').every((item) => item.kind !== 'draft')).toBe(true);
    expect(filterChatActions('/').length).toBe(chatOverflowMenuActions().length);
    expect(filterChatActions('/新建').some((item) => item.id === 'new-session')).toBe(true);
    expect(filterChatActions('/搜索').some((item) => item.id === 'focus-history-search')).toBe(true);
    expect(filterChatActions('/搜索历史会话').some((item) => item.id === 'focus-history-search')).toBe(true);
    expect(filterChatActions('/打开历史会话').some((item) => item.id === 'open-history')).toBe(true);
    expect(filterChatActions('/copy').some((item) => item.id === 'copy-latest-reply')).toBe(true);
    expect(filterChatActions('/报错').some((item) => item.id === 'sample-explain-error')).toBe(true);
    expect(normalizeActionQuery('  新建  ')).toBe('新建');
    expect(actionMatchesQuery(CHAT_ACTIONS[0], 'new')).toBe(true);
  });

  it('mixes runtime commands into slash search', () => {
    const extra = [{ id: 'runtime-model:gpt-spark', kind: 'local' as const, label: '换模型：gpt-spark', keywords: ['model', '模型', 'gpt-spark'] }];
    expect(filterChatActions('/model', extra).map((item) => item.id)).toContain('runtime-model:gpt-spark');
    expect(filterChatActions('\\模型', extra).map((item) => item.id)).toContain('runtime-model:gpt-spark');
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

  it('exposes four draft starters that only fill the composer', () => {
    const starters = chatStarterActions();
    expect(starters).toHaveLength(4);
    expect(starters.every((item) => item.kind === 'draft' && Boolean(item.draftText))).toBe(true);
    expect(starters.map((item) => item.id)).toEqual([
      'sample-understand-project',
      'sample-check-issues',
      'sample-summarize',
      'sample-write-tests',
    ]);
    expect(chatOverflowMenuActions().every((item) => item.kind !== 'draft')).toBe(true);
    expect(chatOverflowMenuActions().some((item) => item.id === 'new-session')).toBe(true);
    expect(CHAT_ACTIONS.some((item) => item.kind === 'draft')).toBe(true);
  });

  it('sits the slash panel tight above the textarea, not mid-viewport', () => {
    expect(slashMenuFixedPosition({
      anchorTop: 640,
      anchorLeft: 48,
      viewportHeight: 800,
    })).toEqual({ left: 48, bottom: 164 });
    expect(slashMenuFixedPosition({
      anchorTop: 400,
      anchorLeft: 20,
      viewportHeight: 800,
      gap: 4,
    }).bottom).toBe(404);
  });
});
