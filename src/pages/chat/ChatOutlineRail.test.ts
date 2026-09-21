import { createElement, type ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import { StorageKey } from '@/lib/storage-key';
import type { ChatMessage } from '@/lib/types';
import type { TurnGroup } from './chat-format';
import { ChatOutlineRail } from './ChatOutlineRail';
import { saveChatOutlineEnabled } from './chat-outline-pref';

function user(id: string, content: string): ChatMessage {
  return {
    id,
    conversationId: 'c1',
    turn: 1,
    role: 'user',
    content,
    status: 'ok',
    durationMs: 0,
    createdAt: '2026-09-20T00:00:00.000Z',
  };
}

function turns(...contents: string[]): TurnGroup[] {
  return contents.map((content, index) => ({
    turn: index + 1,
    user: user(`u${index + 1}`, content),
    agents: [],
  }));
}

function renderRail(props: {
  turns: TurnGroup[];
  measuredWidth: number;
  enabled?: boolean;
}): string {
  return renderToStaticMarkup(
    createElement(
      TooltipProvider,
      null,
      createElement(ChatOutlineRail, {
        turns: props.turns,
        measuredWidth: props.measuredWidth,
        ...(props.enabled === undefined ? {} : { enabled: props.enabled }),
      }),
    ) as ReactElement,
  );
}

function hasOutline(html: string): boolean {
  return html.includes('data-testid="chat-outline-rail"');
}

describe('ChatOutlineRail visibility gates', () => {
  it('shows the rail only when the setting, two user messages, and 768px all hold', () => {
    expect(hasOutline(renderRail({
      turns: turns('first', 'second'),
      measuredWidth: 768,
      enabled: true,
    }))).toBe(true);
    expect(hasOutline(renderRail({
      turns: turns('first', 'second', 'third'),
      measuredWidth: 800,
      enabled: true,
    }))).toBe(true);
  });

  it('hides when any one gate fails', () => {
    expect(hasOutline(renderRail({
      turns: turns('first', 'second'),
      measuredWidth: 768,
      enabled: false,
    }))).toBe(false);
    expect(hasOutline(renderRail({
      turns: turns('only one'),
      measuredWidth: 800,
      enabled: true,
    }))).toBe(false);
    expect(hasOutline(renderRail({
      turns: [],
      measuredWidth: 800,
      enabled: true,
    }))).toBe(false);
    expect(hasOutline(renderRail({
      turns: turns('first', 'second'),
      measuredWidth: 767,
      enabled: true,
    }))).toBe(false);
    expect(hasOutline(renderRail({
      turns: turns('first', 'second'),
      measuredWidth: 0,
      enabled: true,
    }))).toBe(false);
  });

  it('does not count agent-only turns toward the two-prompt gate', () => {
    const oneUser = [
      { turn: 1, user: user('u1', 'only user'), agents: [] },
      { turn: 2, agents: [] },
      { turn: 3, agents: [] },
    ];
    const html = renderRail({ turns: oneUser, measuredWidth: 800, enabled: true });
    expect(hasOutline(html)).toBe(false);
    expect(html).toContain('data-chat-outline-measure');
    expect(html).not.toContain('role="tablist"');
  });

  it('returns nothing when the setting is off, and keeps a measure host for one user message', () => {
    expect(renderRail({
      turns: turns('first', 'second'),
      measuredWidth: 800,
      enabled: false,
    })).toBe('');
    const one = renderRail({ turns: turns('only one'), measuredWidth: 800 });
    expect(hasOutline(one)).toBe(false);
    expect(one).toContain('data-chat-outline-measure');
  });

  describe('stored preference when enabled is omitted', () => {
    const store = new Map<string, string>();

    beforeEach(() => {
      store.clear();
      vi.stubGlobal('localStorage', {
        getItem: (key: string) => store.get(key) ?? null,
        setItem: (key: string, value: string) => {
          store.set(key, value);
        },
        removeItem: (key: string) => {
          store.delete(key);
        },
      });
    });

    afterEach(() => {
      vi.unstubAllGlobals();
    });

    it('defaults on, and hides after the preference is saved off', () => {
      const wideTwo = { turns: turns('first', 'second'), measuredWidth: 768 };
      expect(hasOutline(renderRail(wideTwo))).toBe(true);
      saveChatOutlineEnabled(false);
      expect(store.get(StorageKey.chatOutlineEnabled)).toBe('0');
      expect(hasOutline(renderRail(wideTwo))).toBe(false);
      expect(renderRail(wideTwo)).toBe('');
    });
  });
});

describe('ChatOutlineRail markup', () => {
  it('keeps the measure host mounted for one user message so width can attach', () => {
    const html = renderRail({ turns: turns('only one'), measuredWidth: 800 });
    expect(html).toContain('data-chat-outline-measure');
    expect(html).not.toContain('chat-outline-rail');
    expect(html).not.toContain('role="tablist"');
  });

  it('draws a tablist when two prompts fit a 768px panel', () => {
    const html = renderRail({ turns: turns('first', 'second'), measuredWidth: 768 });
    expect(html).toContain('data-testid="chat-outline-rail"');
    expect(html).toContain('role="tablist"');
    expect(html).toContain('role="tab"');
    expect(html).toContain('type="button"');
    expect(html).toContain('data-testid="chat-outline-tick-u1"');
    expect(html).toContain('data-testid="chat-outline-tick-u2"');
    expect(html).toContain('1 / 2：first');
    expect(html).toContain('2 / 2：second');
    expect(html).toContain('left:8px');
    expect(html).not.toContain('flex-grow:1');
    expect(html).not.toContain('data-testid="chat-outline-preview"');
  });

  it('does not treat a zero-width empty host as a mounted rail', () => {
    const html = renderRail({ turns: turns('first', 'second'), measuredWidth: 0 });
    expect(html).toContain('data-chat-outline-measure');
    expect(html).not.toContain('chat-outline-rail');
    expect(html).not.toContain('role="tablist"');
    expect(html).not.toContain('role="tab"');
    expect(html).not.toContain('chat-outline-tick-');
  });

  it('hides the rail when the panel is narrower than 768px', () => {
    const html = renderRail({ turns: turns('first', 'second'), measuredWidth: 767 });
    expect(html).not.toContain('chat-outline-rail');
  });

  it('hides the rail when the setting is off', () => {
    const html = renderRail({
      turns: turns('first', 'second'),
      measuredWidth: 800,
      enabled: false,
    });
    expect(html).not.toContain('chat-outline-rail');
  });

  it('keeps a measure wrapper when two prompts are too narrow to draw ticks', () => {
    const html = renderRail({ turns: turns('first', 'second'), measuredWidth: 767 });
    expect(html).toContain('pointer-events-none');
    expect(html).not.toContain('data-testid="chat-outline-rail"');
  });

  it('skips agent-only turns and still jumps by the user message id', () => {
    const html = renderRail({
      turns: [
        { turn: 1, user: user('u1', 'first'), agents: [] },
        { turn: 2, agents: [] },
        { turn: 3, user: user('u3', 'third'), agents: [] },
      ],
      measuredWidth: 800,
    });
    expect(html).toContain('data-testid="chat-outline-tick-u1"');
    expect(html).toContain('data-testid="chat-outline-tick-u3"');
    expect(html).not.toContain('chat-outline-tick-u2');
    expect(html).toContain('1 / 2：first');
    expect(html).toContain('2 / 2：third');
    expect(html).not.toContain('data-testid="chat-outline-preview"');
  });
});
