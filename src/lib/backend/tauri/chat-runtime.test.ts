import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { RuntimeSnapshot } from '@/lib/backend/contracts/chat-runtime';
import { createTauriChatPort } from './chat';

const invokeMock = vi.hoisted(() => vi.fn());
vi.mock('./invoke', () => ({ invoke: invokeMock, Channel: class {} }));

beforeEach(() => { invokeMock.mockReset(); });

describe('Tauri durable chat boundary', () => {
  it('preserves the replay cursor, pending request identity and gap indicator', async () => {
    const snapshot: RuntimeSnapshot = {
      conversationId: 'chat-a', enabled: true, runId: 'run-a', phase: 'waiting',
      lastSequence: 92, events: [], gap: true,
      pendingRequests: [{
        id: 'request-a', runId: 'run-a', kind: 'command', title: '执行命令',
        detail: 'echo test', questions: [],
      }],
    };
    invokeMock.mockResolvedValueOnce(snapshot);
    await expect(createTauriChatPort().runtimeSnapshot('chat-a', 7)).resolves.toEqual(snapshot);
    expect(invokeMock).toHaveBeenCalledWith('chat_runtime_snapshot', {
      conversationId: 'chat-a', afterSequence: 7,
    });
  });

  it('forwards start, reply, steer and stop identities without losing nested answers', async () => {
    const port = createTauriChatPort();
    await port.runtimeStart('chat-a', '你好', 'send-1');
    const reply = {
      conversationId: 'chat-a', runId: 'run-a', requestId: 'question-1',
      clientRequestId: 'answer-1', answers: { language: ['中文'] },
    };
    await port.runtimeReply(reply);
    await port.runtimeSteer('chat-a', 'run-a', '只解释', 'steer-1');
    await port.runtimeCancel('chat-a', 'run-a');
    expect(invokeMock.mock.calls).toEqual([
      ['chat_runtime_start', { conversationId: 'chat-a', prompt: '你好', clientRequestId: 'send-1' }],
      ['chat_runtime_reply', { reply }],
      ['chat_runtime_steer', { conversationId: 'chat-a', runId: 'run-a', prompt: '只解释', clientRequestId: 'steer-1' }],
      ['chat_runtime_cancel', { conversationId: 'chat-a', runId: 'run-a' }],
    ]);
  });

  it('propagates unavailable and stale-request errors without attempting legacy send', async () => {
    const port = createTauriChatPort();
    invokeMock.mockRejectedValue(new Error('runtime unavailable'));
    await expect(port.runtimeSnapshot('chat-a')).rejects.toThrow('runtime unavailable');
    await expect(port.runtimeReply({
      conversationId: 'chat-a', runId: 'old-run', requestId: 'old-request',
      clientRequestId: 'answer-2', decision: 'allow',
    })).rejects.toThrow('runtime unavailable');
    expect(invokeMock.mock.calls.map(([command]) => command)).toEqual([
      'chat_runtime_snapshot', 'chat_runtime_reply',
    ]);
  });
});
