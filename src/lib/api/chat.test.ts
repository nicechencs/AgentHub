import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { resetBackend } from '@/app/runtime';
import { resetChatMock } from '@/dev/mocks/chat';
import {
  chatCancel,
  chatSend,
  createConversation,
  deleteConversation,
  listChatMessages,
  listConversations,
  mapChatMessage,
  mapConversation,
  updateConversation,
} from '@/lib/api/chat';
import type { ChatEvent } from '@/lib/types';

describe('chat API (browser mock)', () => {
  beforeEach(() => {
    resetBackend();
    resetChatMock();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    resetChatMock();
    resetBackend();
  });

  it('mapConversation / mapChatMessage are identity copies', () => {
    const c = {
      id: 'c1',
      title: 't',
      agentIds: ['claude' as const],
      cwd: null,
      allowDangerous: false,
      createdAt: 'a',
      updatedAt: 'b',
    };
    expect(mapConversation(c)).toEqual(c);
    expect(mapConversation(c)).not.toBe(c);

    const m = {
      id: 'm1',
      conversationId: 'c1',
      turn: 1,
      role: 'user' as const,
      content: 'hi',
      status: 'ok' as const,
      durationMs: 0,
      createdAt: 't',
    };
    expect(mapChatMessage(m)).toEqual(m);
    expect(mapChatMessage(m)).not.toBe(m);
  });

  it('create / list / update / delete conversation', async () => {
    const createP = createConversation(['claude', 'codex'], 'D:\\demo');
    await vi.runAllTimersAsync();
    const conv = await createP;
    expect(conv.id).toMatch(/^conv-mock-/);
    expect(conv.agentIds).toEqual(['claude', 'codex']);
    expect(conv.cwd).toBe('D:\\demo');
    expect(conv.title).toBe('');

    const listP = listConversations();
    await vi.runAllTimersAsync();
    expect((await listP).map((c) => c.id)).toEqual([conv.id]);

    const updP = updateConversation(conv.id, {
      title: 'hello',
      allowDangerous: true,
      cwd: null,
    });
    await vi.runAllTimersAsync();
    const updated = await updP;
    expect(updated.title).toBe('hello');
    expect(updated.allowDangerous).toBe(true);
    expect(updated.cwd).toBeNull();

    const delP = deleteConversation(conv.id);
    await vi.runAllTimersAsync();
    await delP;
    const list2P = listConversations();
    await vi.runAllTimersAsync();
    expect(await list2P).toEqual([]);
  });

  it('chatSend streams events with turn and persists messages', async () => {
    const createP = createConversation(['claude']);
    await vi.runAllTimersAsync();
    const conv = await createP;

    const events: ChatEvent[] = [];
    const sendP = chatSend(conv.id, 'first question', (ev) => events.push(ev));
    await vi.runAllTimersAsync();
    await sendP;

    expect(events[0]).toMatchObject({ type: 'started', turn: 1, agents: ['claude'] });
    expect(events.some((e) => e.type === 'agentStarted' && e.turn === 1)).toBe(true);
    expect(
      events.some((e) => e.type === 'agentChunk' && e.stream === 'stdout' && e.turn === 1),
    ).toBe(true);
    expect(
      events.some((e) => e.type === 'agentChunk' && e.stream === 'stderr' && e.turn === 1),
    ).toBe(true);
    expect(
      events.some(
        (e) => e.type === 'agentProcess' && e.step.type === 'tool' && e.turn === 1,
      ),
    ).toBe(true);
    expect(events.some((e) => e.type === 'agentFinished' && e.turn === 1)).toBe(true);
    expect(events.at(-1)).toMatchObject({ type: 'finished', turn: 1, ok: true });

    const msgsP = listChatMessages(conv.id);
    await vi.runAllTimersAsync();
    const msgs = await msgsP;
    expect(msgs.some((m) => m.role === 'user' && m.content === 'first question')).toBe(true);
    expect(msgs.filter((m) => m.role === 'agent')).toHaveLength(1);
    expect(msgs.every((m) => m.status !== 'running')).toBe(true);

    const listP = listConversations();
    await vi.runAllTimersAsync();
    const listed = await listP;
    expect(listed[0]?.title).toContain('first');
  });

  it('chatCancel mid-send marks agent cancelled', async () => {
    const createP = createConversation(['claude', 'codex']);
    await vi.runAllTimersAsync();
    const conv = await createP;

    const events: ChatEvent[] = [];
    const sendP = chatSend(conv.id, 'to cancel', (ev) => events.push(ev));

    await vi.advanceTimersByTimeAsync(50);
    await chatCancel(conv.id);
    await vi.runAllTimersAsync();
    await sendP;

    const finished = events.filter((e) => e.type === 'agentFinished');
    expect(finished.length).toBeGreaterThanOrEqual(1);
    expect(
      finished.some(
        (e) => e.type === 'agentFinished' && e.message.status === 'cancelled',
      ),
    ).toBe(true);

    const last = events.at(-1);
    expect(last?.type).toBe('finished');
    if (last?.type === 'finished') {
      expect(last.ok).toBe(false);
    }
  });

  it('chatSend rejects missing conversation', async () => {
    const sendP = chatSend('missing', 'x', () => {});
    await expect(sendP).rejects.toThrow(/not found/);
  });

  it('second turn increments turn number', async () => {
    const createP = createConversation(['claude']);
    await vi.runAllTimersAsync();
    const conv = await createP;

    const send1 = chatSend(conv.id, 'one', () => {});
    await vi.runAllTimersAsync();
    await send1;

    const turns: number[] = [];
    const send2 = chatSend(conv.id, 'two', (ev) => {
      if (ev.type === 'started') turns.push(ev.turn);
    });
    await vi.runAllTimersAsync();
    await send2;
    expect(turns).toEqual([2]);
  });
});
