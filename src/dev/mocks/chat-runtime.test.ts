import { beforeEach, describe, expect, it } from 'vitest';
import { createMockChatPort, resetChatMock } from './chat';

describe('mock chat runtime', () => {
  beforeEach(resetChatMock);

  it('enables a new Codex conversation and preserves its run id for cancellation', async () => {
    const chat = createMockChatPort();
    const conversation = await chat.createConversation(['codex']);
    await expect(chat.runtimeSnapshot(conversation.id)).resolves.toMatchObject({
      enabled: true,
      phase: 'idle',
    });
    const started = await chat.runtimeStart(conversation.id, 'read the project', 'client-1');
    expect(started.runId).toBeTruthy();
    expect(started.events).toHaveLength(1);
    expect(started.currentMessage).toMatchObject({
      conversationId: conversation.id,
      status: 'running',
      content: '',
    });
    await chat.runtimeCancel(conversation.id, started.runId!);
    await expect(chat.runtimeSnapshot(conversation.id)).resolves.toMatchObject({
      runId: started.runId,
      phase: 'cancelling',
      currentMessage: { status: 'cancelled' },
    });
  });

  it('keeps a non-Codex conversation on the legacy path', async () => {
    const chat = createMockChatPort();
    const conversation = await chat.createConversation(['claude']);
    await expect(chat.runtimeSnapshot(conversation.id)).resolves.toMatchObject({ enabled: false });
  });

  it('prefers a warmed catalog during an active turn and stays empty when never fetched', async () => {
    const chat = createMockChatPort();
    const conversation = await chat.createConversation(['codex']);
    const warmed = await chat.runtimeOptions(conversation.id);
    expect(warmed.models.length).toBeGreaterThan(0);
    const started = await chat.runtimeStart(conversation.id, 'go', 'client-warm');
    expect(started.phase).not.toBe('idle');
    await expect(chat.runtimeOptions(conversation.id)).resolves.toMatchObject({
      settingsFrozen: true,
      models: warmed.models,
      extensions: warmed.extensions,
    });

    resetChatMock();
    const chat2 = createMockChatPort();
    const cold = await chat2.createConversation(['codex']);
    const running = await chat2.runtimeStart(cold.id, 'go', 'client-cold');
    expect(running.phase).not.toBe('idle');
    await expect(chat2.runtimeOptions(cold.id)).resolves.toMatchObject({
      settingsFrozen: true,
      models: [],
      extensions: [],
    });
  });
});


  it('rejects unsupported model×effort pairs and fills default on model switch', async () => {
    const chat = createMockChatPort();
    const conversation = await chat.createConversation(['codex']);
    await chat.runtimeOptions(conversation.id);

    await expect(
      chat.runtimeSetSettings(conversation.id, {
        model: 'gpt-5.3-codex-spark',
        effort: 'medium',
      }),
    ).rejects.toThrow(/不支持思考强度/);

    const switched = await chat.runtimeSetSettings(conversation.id, {
      model: 'gpt-5.3-codex-spark',
      effort: null,
    });
    expect(switched).toMatchObject({
      model: 'gpt-5.3-codex-spark',
      effort: 'low',
    });

    const started = await chat.runtimeStart(conversation.id, 'ping', 'client-effort-ok');
    expect(started.phase).not.toBe('idle');
  });
