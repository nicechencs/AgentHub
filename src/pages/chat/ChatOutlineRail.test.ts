import { createElement, type ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { ChatMessage } from '@/lib/types';
import type { TurnGroup } from './chat-format';
import { ChatOutlineRail } from './ChatOutlineRail';

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
    createElement(ChatOutlineRail, {
      turns: props.turns,
      measuredWidth: props.measuredWidth,
      enabled: props.enabled ?? true,
    }) as ReactElement,
  );
}

describe('ChatOutlineRail markup', () => {
  it('does not draw a rail for one user message', () => {
    const html = renderRail({ turns: turns('only one'), measuredWidth: 800 });
    expect(html).not.toContain('chat-outline-rail');
    expect(html).not.toContain('role="tablist"');
  });

  it('draws a tablist when two prompts fit a 720px panel', () => {
    const html = renderRail({ turns: turns('first', 'second'), measuredWidth: 720 });
    expect(html).toContain('data-testid="chat-outline-rail"');
    expect(html).toContain('role="tablist"');
    expect(html).toContain('role="tab"');
    expect(html).toContain('data-testid="chat-outline-tick-u1"');
    expect(html).toContain('data-testid="chat-outline-tick-u2"');
    expect(html).toContain('1 / 2：first');
    expect(html).toContain('2 / 2：second');
  });

  it('hides the rail when the panel is narrower than 720px', () => {
    const html = renderRail({ turns: turns('first', 'second'), measuredWidth: 719 });
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
});
