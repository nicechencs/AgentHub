import { describe, expect, it } from 'vitest';
import type { ChatMessage } from '@/lib/types';
import type { TurnGroup } from './chat-format';
import {
  OUTLINE_MAGNIFY_RADIUS,
  applyOutlineJumpOffset,
  outlinePromptPreview,
  outlinePromptsFromTurns,
  outlineTickSize,
  planOutlineJumpScroll,
  promptTickMagnification,
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
});

describe('outlinePromptPreview', () => {
  it('collapses whitespace and clips at 120 characters', () => {
    expect(outlinePromptPreview('  hello   \n  world  ')).toBe('hello world');
    expect(outlinePromptPreview('a'.repeat(120))).toBe('a'.repeat(120));
    expect(outlinePromptPreview('a'.repeat(121))).toBe(`${'a'.repeat(120)}…`);
    expect(outlinePromptPreview('   \n\t  ')).toBe('');
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
});

describe('shouldShowChatOutline', () => {
  it('requires the setting, two prompts, and a 720px panel', () => {
    expect(shouldShowChatOutline({ enabled: true, promptCount: 2, panelWidth: 720 })).toBe(true);
    expect(shouldShowChatOutline({ enabled: true, promptCount: 1, panelWidth: 900 })).toBe(false);
    expect(shouldShowChatOutline({ enabled: true, promptCount: 2, panelWidth: 719 })).toBe(false);
    expect(shouldShowChatOutline({ enabled: false, promptCount: 5, panelWidth: 900 })).toBe(false);
  });
});

describe('outlineTickSize', () => {
  it('grows from the resting or current width toward the magnified size', () => {
    expect(outlineTickSize(false, 0)).toEqual({ width: 10, height: 2 });
    expect(outlineTickSize(true, 0)).toEqual({ width: 18, height: 2 });
    expect(outlineTickSize(false, 1)).toEqual({ width: 26, height: 4 });
    expect(outlineTickSize(true, 1)).toEqual({ width: 26, height: 4 });
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
});

describe('outline jump scroll', () => {
  it('places the target 8px below the container top', () => {
    expect(planOutlineJumpScroll(80, 40, 200)).toBe(152);
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
