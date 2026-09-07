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

  it('rejects upgrading Kiro history without changing its runtime snapshot', async () => {
    const chat = createMockChatPort();
    const conversation = await chat.createConversation(['kiro']);
    const before = await chat.runtimeSnapshot(conversation.id);
    await expect(chat.runtimeContinueLegacy(conversation.id)).rejects.toThrow(
      '这条 Kiro 对话不能切换聊天方式，请新建对话',
    );
    await expect(chat.runtimeSnapshot(conversation.id)).resolves.toEqual(before);
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

  it('rebuilds an idle catalog when refresh is requested', async () => {
    const chat = createMockChatPort();
    const conversation = await chat.createConversation(['codex']);
    const first = await chat.runtimeOptions(conversation.id);
    expect(first.models.length).toBeGreaterThan(0);
    await expect(chat.runtimeOptions(conversation.id, { refresh: true })).resolves.toMatchObject({
      settingsFrozen: false,
      conversationId: conversation.id,
    });
  });
});


  it('defaults spark on switch; learns to hide over-reported medium after start reject', async () => {
    const chat = createMockChatPort();
    const conversation = await chat.createConversation(['codex']);
    const catalog = await chat.runtimeOptions(conversation.id);
    const spark = catalog.models.find((item) => item.id === 'gpt-5.3-codex-spark');
    expect(spark?.efforts).toEqual(['low', 'medium', 'high', 'xhigh']);

    const switched = await chat.runtimeSetSettings(conversation.id, {
      model: 'gpt-5.3-codex-spark',
      effort: null,
    });
    expect(switched).toMatchObject({
      model: 'gpt-5.3-codex-spark',
      effort: 'high',
    });

    // Catalog over-reports medium, so setSettings still accepts it before learning.
    await expect(
      chat.runtimeSetSettings(conversation.id, {
        model: 'gpt-5.3-codex-spark',
        effort: 'medium',
      }),
    ).resolves.toMatchObject({ effort: 'medium' });

    await expect(chat.runtimeStart(conversation.id, 'ping', 'client-spark-medium')).rejects.toThrow(
      /reasoningEffort/,
    );

    const after = await chat.runtimeOptions(conversation.id);
    expect(after.models.find((item) => item.id === 'gpt-5.3-codex-spark')?.efforts).toEqual([
      'low',
      'high',
      'xhigh',
    ]);
    await expect(
      chat.runtimeSetSettings(conversation.id, {
        model: 'gpt-5.3-codex-spark',
        effort: 'medium',
      }),
    ).rejects.toThrow(/不支持思考强度/);

    const ok = await chat.runtimeSetSettings(conversation.id, {
      model: 'gpt-5.3-codex-spark',
      effort: 'high',
    });
    const started = await chat.runtimeStart(conversation.id, 'ping', 'client-effort-ok');
    expect(ok.effort).toBe('high');
    expect(started.phase).not.toBe('idle');
  });
