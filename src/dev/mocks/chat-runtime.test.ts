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
});
