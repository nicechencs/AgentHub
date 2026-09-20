import { createElement, type ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import type { Conversation } from '@/lib/types';
import { ChatSessionHeader } from './ChatSessionHeader';
import {
  adjacentSessionId,
  chatSessionSwitchShortcutAction,
  sessionSwitchNeighbors,
} from './chat-session-switch';

vi.mock('@/components/shared/LanguageProvider', async () => {
  const { createTranslator } = await import('@/lib/i18n');
  const t = createTranslator('zh');
  return {
    useI18n: () => ({ lang: 'zh', setLanguage: () => undefined, t }),
  };
});

vi.mock('@/components/ui/toast', () => ({
  useToast: () => ({ toast: () => undefined }),
}));

vi.mock('@/components/layout/ChromeActions', () => ({
  ChromeActions: () => null,
}));

const sessions = [{ id: 'a' }, { id: 'b' }, { id: 'c' }];

describe('adjacentSessionId', () => {
  it('returns null for an empty list', () => {
    expect(adjacentSessionId([], 'a', 'next')).toBeNull();
    expect(adjacentSessionId([], null, 'prev')).toBeNull();
  });

  it('cannot switch a single-item list that is already focused', () => {
    expect(adjacentSessionId([{ id: 'a' }], 'a', 'next')).toBeNull();
    expect(adjacentSessionId([{ id: 'a' }], 'a', 'prev')).toBeNull();
  });

  it('focuses the only session when the current id is missing', () => {
    expect(adjacentSessionId([{ id: 'a' }], null, 'next')).toBe('a');
    expect(adjacentSessionId([{ id: 'a' }], 'gone', 'prev')).toBe('a');
  });

  it('walks the flat list and wraps at both ends', () => {
    expect(adjacentSessionId(sessions, 'a', 'next')).toBe('b');
    expect(adjacentSessionId(sessions, 'b', 'next')).toBe('c');
    expect(adjacentSessionId(sessions, 'c', 'next')).toBe('a');
    expect(adjacentSessionId(sessions, 'a', 'prev')).toBe('c');
    expect(adjacentSessionId(sessions, 'b', 'prev')).toBe('a');
    expect(adjacentSessionId(sessions, 'c', 'prev')).toBe('b');
  });

  it('lands on the first or last session when the current id is not in the list', () => {
    expect(adjacentSessionId(sessions, null, 'next')).toBe('a');
    expect(adjacentSessionId(sessions, 'gone', 'prev')).toBe('c');
  });
});

describe('sessionSwitchNeighbors', () => {
  it('exposes both wrapped neighbors', () => {
    expect(sessionSwitchNeighbors(sessions, 'b')).toEqual({ prevId: 'a', nextId: 'c' });
    expect(sessionSwitchNeighbors(sessions, 'a')).toEqual({ prevId: 'c', nextId: 'b' });
    expect(sessionSwitchNeighbors([{ id: 'a' }], 'a')).toEqual({ prevId: null, nextId: null });
  });
});

describe('chatSessionSwitchShortcutAction', () => {
  const base = {
    key: '',
    altKey: true,
    metaKey: false,
    ctrlKey: false,
    shiftKey: false,
    overlayOpen: false,
  };

  it('maps Alt+ArrowUp / Alt+ArrowDown', () => {
    expect(chatSessionSwitchShortcutAction({ ...base, key: 'ArrowUp' })).toBe('prev');
    expect(chatSessionSwitchShortcutAction({ ...base, key: 'ArrowDown' })).toBe('next');
    expect(
      chatSessionSwitchShortcutAction({ ...base, key: 'Unidentified', code: 'ArrowUp' }),
    ).toBe('prev');
  });

  it('ignores chords that already belong to Chat', () => {
    expect(chatSessionSwitchShortcutAction({ ...base, key: 'k', ctrlKey: true, altKey: false })).toBeNull();
    expect(chatSessionSwitchShortcutAction({ ...base, key: 'n', ctrlKey: true, altKey: false })).toBeNull();
    expect(chatSessionSwitchShortcutAction({ ...base, key: 'Enter', altKey: false })).toBeNull();
    expect(chatSessionSwitchShortcutAction({ ...base, key: 'Escape', altKey: false })).toBeNull();
    expect(chatSessionSwitchShortcutAction({ ...base, key: '/', altKey: false })).toBeNull();
    expect(chatSessionSwitchShortcutAction({ ...base, key: 'ArrowUp', altKey: false })).toBeNull();
    expect(chatSessionSwitchShortcutAction({ ...base, key: 'ArrowUp', ctrlKey: true })).toBeNull();
    expect(chatSessionSwitchShortcutAction({ ...base, key: 'ArrowUp', overlayOpen: true })).toBeNull();
  });
});

function conv(partial?: Partial<Conversation>): Conversation {
  return {
    id: 'a',
    title: '修登录',
    agentIds: ['claude'],
    cwd: 'D:\\work\\agenthub',
    allowDangerous: false,
    createdAt: '2026-09-20T00:00:00.000Z',
    updatedAt: '2026-09-20T00:00:00.000Z',
    ...partial,
  };
}

function header(partial?: Partial<Parameters<typeof ChatSessionHeader>[0]>): ReactElement {
  const active = conv();
  return createElement(ChatSessionHeader, {
    active,
    railOpen: false,
    sessions: [active, conv({ id: 'b', title: '下一场', cwd: 'D:\\work\\other' })],
    sendingConversationIds: ['a'],
    onExpandRail: () => undefined,
    onRename: async () => true,
    onFocus: () => undefined,
    onOpenSettings: () => undefined,
    onPickWorkingDirectory: () => undefined,
    ...partial,
  });
}

describe('collapsed session switcher', () => {
  it('shows the current title, working-directory short name, and a sending dot', () => {
    const html = renderToStaticMarkup(createElement(TooltipProvider, null, header()));
    expect(html).toContain('data-help="chat-session-switch"');
    expect(html).toContain('修登录');
    expect(html).toContain('agenthub');
    expect(html).toContain('data-sending=""');
    expect(html).toContain('上一条会话');
    expect(html).toContain('下一条会话');
  });

  it('keeps the rename title when the history rail is open', () => {
    const html = renderToStaticMarkup(createElement(TooltipProvider, null, header({ railOpen: true })));
    expect(html).not.toContain('data-help="chat-session-switch"');
    expect(html).toContain('修登录');
  });

  it('does not show connect copy in the header', () => {
    const html = renderToStaticMarkup(createElement(TooltipProvider, null, header({ railOpen: true })));
    expect(html).not.toContain('这次对话怎么接');
    expect(html).not.toContain('data-help="chat-session-connect"');
    expect(html).toContain('data-help="chat-settings"');
    expect(html).toContain('会话设置');
  });
});
