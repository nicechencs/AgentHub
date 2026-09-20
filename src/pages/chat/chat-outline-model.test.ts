import { describe, expect, it } from 'vitest';
import type { ChatMessage } from '@/lib/types';
import type { TurnGroup } from './chat-format';
import {
  OUTLINE_MAGNIFY_RADIUS,
  OUTLINE_MIN_PANEL_WIDTH_PX,
  OUTLINE_MIN_PROMPTS,
  applyOutlineJumpOffset,
  outlinePanelElement,
  outlinePromptPreview,
  outlinePromptsFromTurns,
  outlineTickSize,
  planOutlineJumpScroll,
  promptTickMagnification,
  readOutlinePanelWidth,
  resolveActivePromptId,
  shouldShowChatOutline,
  type ChatOutlinePrompt,
} from './chat-outline-model';

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

function prompt(id: string): ChatOutlinePrompt {
  return { id, preview: id };
}

describe('promptTickMagnification', () => {
  it('peaks under the pointer and decays to nothing at the radius', () => {
    expect(promptTickMagnification(0)).toBe(1);
    expect(promptTickMagnification(OUTLINE_MAGNIFY_RADIUS)).toBe(0);
    expect(promptTickMagnification(OUTLINE_MAGNIFY_RADIUS + 10)).toBe(0);
  });

  it('falls off monotonically and symmetrically around the pointer', () => {
    const above = [0, 1, 2, 3].map((distance) => promptTickMagnification(distance));
    const below = [0, -1, -2, -3].map((distance) => promptTickMagnification(distance));

    expect(above).toEqual(below);
    expect(above).toEqual([...above].sort((left, right) => right - left));
  });

  it('treats non-finite distances as outside the bulge', () => {
    expect(promptTickMagnification(Number.NaN)).toBe(0);
    expect(promptTickMagnification(Number.POSITIVE_INFINITY)).toBe(0);
  });
});

describe('outlinePromptPreview', () => {
  it('collapses whitespace and clips at 120 characters', () => {
    expect(outlinePromptPreview('  hello   \n  world  ')).toBe('hello world');
    expect(outlinePromptPreview('a'.repeat(120))).toBe('a'.repeat(120));
    expect(outlinePromptPreview('a'.repeat(121))).toBe(`${'a'.repeat(120)}…`);
    expect(outlinePromptPreview('   \n\t  ')).toBe('');
  });

  it('honors a custom clip limit', () => {
    expect(outlinePromptPreview('abcdefghij', 6)).toBe('abcdef…');
    expect(outlinePromptPreview('short', 6)).toBe('short');
  });
});

describe('outlinePromptsFromTurns', () => {
  it('keeps user messages in turn order and skips agent-only turns', () => {
    const turns: TurnGroup[] = [
      { turn: 1, user: user('u1', 'first'), agents: [] },
      { turn: 2, agents: [] },
      { turn: 3, user: user('u2', 'second  line'), agents: [] },
    ];
    expect(outlinePromptsFromTurns(turns)).toEqual([
      { id: 'u1', preview: 'first' },
      { id: 'u2', preview: 'second line' },
    ]);
  });

  it('returns nothing when every turn is agent-only or empty', () => {
    expect(outlinePromptsFromTurns([])).toEqual([]);
    expect(outlinePromptsFromTurns([{ turn: 1, agents: [] }])).toEqual([]);
  });
});

describe('shouldShowChatOutline', () => {
  it('locks the gates at preference on, two user prompts, and 720px', () => {
    expect(OUTLINE_MIN_PROMPTS).toBe(2);
    expect(OUTLINE_MIN_PANEL_WIDTH_PX).toBe(720);
  });

  it('shows only when preference, prompt count, and panel width all hold', () => {
    const shown = { enabled: true, promptCount: 2, panelWidth: 720 };
    expect(shouldShowChatOutline(shown)).toBe(true);
    expect(shouldShowChatOutline({ ...shown, promptCount: 3, panelWidth: 721 })).toBe(true);

    expect(shouldShowChatOutline({ ...shown, enabled: false })).toBe(false);
    expect(shouldShowChatOutline({ ...shown, promptCount: 0 })).toBe(false);
    expect(shouldShowChatOutline({ ...shown, promptCount: 1 })).toBe(false);
    expect(shouldShowChatOutline({ ...shown, panelWidth: 0 })).toBe(false);
    expect(shouldShowChatOutline({ ...shown, panelWidth: 719 })).toBe(false);

    expect(shouldShowChatOutline({ enabled: false, promptCount: 1, panelWidth: 719 })).toBe(false);
    expect(shouldShowChatOutline({ enabled: true, promptCount: 5, panelWidth: 719 })).toBe(false);
    expect(shouldShowChatOutline({ enabled: false, promptCount: 5, panelWidth: 900 })).toBe(false);
  });
});

describe('outline panel measurement', () => {
  it('prefers the chat stage ancestor over the inner host', () => {
    const stage = { id: 'stage' };
    const host = {
      closest: (selector: string) => (selector === '[data-chat-stage]' ? stage : null),
    };
    expect(outlinePanelElement(host as unknown as Element)).toBe(stage);
  });

  it('falls back to the host when no stage ancestor exists', () => {
    const host = { closest: () => null };
    expect(outlinePanelElement(host as unknown as Element)).toBe(host);
    expect(outlinePanelElement(null)).toBeNull();
  });

  it('reads the stage width used for the mount gate', () => {
    const stage = {
      closest: () => stage,
      getBoundingClientRect: () => ({ width: 1100 }),
    };
    expect(readOutlinePanelWidth(stage as unknown as Element)).toBe(1100);
    expect(readOutlinePanelWidth(null)).toBe(0);
  });
});

describe('outlineTickSize', () => {
  it('grows from the resting or current width toward the magnified size', () => {
    expect(outlineTickSize(false, 0)).toEqual({ width: 10, height: 2 });
    expect(outlineTickSize(true, 0)).toEqual({ width: 18, height: 2 });
    expect(outlineTickSize(false, 1)).toEqual({ width: 26, height: 4 });
    expect(outlineTickSize(true, 1)).toEqual({ width: 26, height: 4 });
    expect(outlineTickSize(false, 0.5)).toEqual({ width: 18, height: 3 });
    expect(outlineTickSize(true, 0.5)).toEqual({ width: 22, height: 3 });
  });
});

describe('resolveActivePromptId', () => {
  const prompts = [prompt('a'), prompt('b'), prompt('c')];

  it('marks the last prompt whose top sits on or above the reading line', () => {
    expect(resolveActivePromptId(prompts, [100, 200, 300], 200)).toBe('b');
    expect(resolveActivePromptId(prompts, [100, 200, 300], 300)).toBe('c');
    expect(resolveActivePromptId(prompts, [100, 200, 300], 400)).toBe('c');
  });

  it('marks nothing above the first prompt', () => {
    expect(resolveActivePromptId(prompts, [100, 200, 300], 50)).toBeNull();
    expect(resolveActivePromptId([], [], 100)).toBeNull();
  });

  it('skips missing or non-finite tops and keeps the last valid mark', () => {
    expect(resolveActivePromptId(prompts, [100, Number.NaN, 300], 300)).toBe('c');
    expect(resolveActivePromptId(prompts, [undefined as unknown as number, 200], 200)).toBe('b');
    expect(resolveActivePromptId(prompts, [Number.POSITIVE_INFINITY, 200, 50], 80)).toBe('c');
  });
});

describe('outline jump scroll', () => {
  it('places the target 8px below the container top', () => {
    expect(planOutlineJumpScroll(80, 40, 200)).toBe(152);
    expect(planOutlineJumpScroll(80, 40, 200, 0)).toBe(160);
  });

  it('writes that offset onto the container after a start-aligned jump', () => {
    const container = {
      scrollTop: 120,
      getBoundingClientRect: () => ({ top: 64 }) as DOMRect,
    };
    const target = {
      getBoundingClientRect: () => ({ top: 64 }) as DOMRect,
    };
    applyOutlineJumpOffset(container, target);
    expect(container.scrollTop).toBe(112);
  });
});
