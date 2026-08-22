import { describe, expect, it } from 'vitest';
import type { Conversation } from '@/lib/types';
import { conversationListState, createSingleFlight, isCurrentChatRequest } from './use-chat-page';

describe('chat async request guards', () => {
  it('rejects a send continuation after switching away and back', () => {
    expect(isCurrentChatRequest('chat-a', 3, 'chat-a', 2)).toBe(false);
    expect(isCurrentChatRequest('chat-a', 3, 'chat-b', 3)).toBe(false);
    expect(isCurrentChatRequest('chat-a', 3, 'chat-a', 3)).toBe(true);
  });

  it('accepts the completion refresh under the generation created by switching back', () => {
    const sendGeneration = 3;
    const currentGenerationAfterReturn = 4;
    expect(
      isCurrentChatRequest('chat-a', currentGenerationAfterReturn, 'chat-a', sendGeneration),
    ).toBe(false);
    expect(
      isCurrentChatRequest(
        'chat-a',
        currentGenerationAfterReturn,
        'chat-a',
        currentGenerationAfterReturn,
      ),
    ).toBe(true);
  });

  it('commits existing and default list selections as one state value', () => {
    const existing: Conversation[] = [
      {
        id: 'existing',
        title: 'Existing',
        agentIds: ['claude'],
        allowDangerous: false,
        createdAt: '',
        updatedAt: '',
      },
    ];
    expect(conversationListState(existing)).toEqual({
      conversations: existing,
      activeId: 'existing',
    });
    expect(conversationListState([])).toEqual({ conversations: [], activeId: null });
  });

  it('shares initialization and allows a later retry after the first settles', async () => {
    let resolveFirst: ((value: string[]) => void) | undefined;
    let calls = 0;
    const run = createSingleFlight<string[]>();
    const factory = () => {
      calls += 1;
      return new Promise<string[]>((resolve) => {
        resolveFirst = resolve;
      });
    };

    const first = run(factory);
    const replay = run(factory);
    expect(replay).toBe(first);
    expect(calls).toBe(1);

    resolveFirst?.(['created-once']);
    await expect(first).resolves.toEqual(['created-once']);

    const retry = run(() => {
      calls += 1;
      return Promise.resolve(['created-after-settle']);
    });
    await expect(retry).resolves.toEqual(['created-after-settle']);
    expect(calls).toBe(2);
  });
});
