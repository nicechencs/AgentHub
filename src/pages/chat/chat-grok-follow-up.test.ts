import { describe, expect, it } from 'vitest';
import {
  appendQueuedFollowUp,
  chatBusySendMode,
  clearQueuedFollowUps,
  grokCanQueueFollowUp,
  grokLegacyContinueKind,
  grokShouldFlushFollowUp,
  prependQueuedFollowUp,
  queuedFollowUpCount,
  queuedFollowUpItems,
  removeQueuedFollowUp,
  restoreQueuedFollowUpOnCancel,
  shiftQueuedFollowUp,
} from './chat-grok-follow-up';

describe('busy composer send', () => {
  it('steers only when a live Codex-style run can inject now', () => {
    expect(chatBusySendMode({ sending: false })).toBeNull();
    expect(
      chatBusySendMode({
        sending: true,
        runtimeEnabled: true,
        steer: true,
        runId: 'run-1',
        phase: 'running',
      }),
    ).toBe('steer');
    expect(
      chatBusySendMode({
        sending: true,
        runtimeEnabled: true,
        steer: true,
        runId: 'run-1',
        phase: 'starting',
      }),
    ).toBe('steer');
  });

  it('queues the next line when steer is unavailable', () => {
    expect(chatBusySendMode({ sending: true })).toBe('queue');
    expect(
      chatBusySendMode({
        sending: true,
        runtimeEnabled: true,
        steer: false,
        runId: 'run-1',
        phase: 'running',
      }),
    ).toBe('queue');
    expect(
      chatBusySendMode({
        sending: true,
        runtimeEnabled: true,
        steer: true,
        runId: null,
        phase: 'starting',
      }),
    ).toBe('queue');
    expect(
      chatBusySendMode({
        sending: true,
        runtimeEnabled: true,
        steer: true,
        runId: 'run-1',
        phase: 'idle',
      }),
    ).toBe('queue');
    expect(
      chatBusySendMode({
        sending: true,
        runtimeEnabled: true,
        steer: true,
        runId: 'run-1',
        phase: 'running',
        queued: true,
      }),
    ).toBe('queue');
  });
});

describe('queued follow-up items', () => {
  it('keeps second and third follow-ups as separate items', () => {
    const queued = appendQueuedFollowUp(appendQueuedFollowUp([], '第二条', 'q-2'), '第三条', 'q-3');
    expect(queuedFollowUpItems(queued).map((item) => item.text)).toEqual(['第二条', '第三条']);
    expect(queued.every((item) => !item.text.includes('；'))).toBe(true);
    const first = shiftQueuedFollowUp(queued);
    expect(first?.next).toEqual({ id: 'q-2', text: '第二条' });
    expect(first?.rest).toEqual([{ id: 'q-3', text: '第三条' }]);
    expect(shiftQueuedFollowUp(first?.rest ?? [])).toEqual({
      next: { id: 'q-3', text: '第三条' },
      rest: [],
    });
    expect(
      prependQueuedFollowUp([{ id: 'q-3', text: '第三条' }], '第二条', 'q-2').map((item) => item.text),
    ).toEqual(['第二条', '第三条']);
    expect(appendQueuedFollowUp([{ id: 'q-1', text: '已排队' }], '  ')).toEqual([
      { id: 'q-1', text: '已排队' },
    ]);
    expect(queuedFollowUpItems([])).toEqual([]);
    expect(queuedFollowUpCount([])).toBe(0);
    expect(queuedFollowUpCount(queued)).toBe(2);
    expect(queuedFollowUpCount([{ id: 'blank', text: '  ' }, { id: 'q-3', text: '第三条' }])).toBe(1);
    expect(shiftQueuedFollowUp([])).toBeNull();
  });

  it('cancels one item without joining the rest', () => {
    const queued = appendQueuedFollowUp(
      appendQueuedFollowUp([{ id: 'q-1', text: '第一条' }], '第二条', 'q-2'),
      '第三条',
      'q-3',
    );
    expect(removeQueuedFollowUp(queued, 'q-2').map((item) => item.text)).toEqual(['第一条', '第三条']);
    expect(removeQueuedFollowUp(queued, 'missing')).toEqual(queued);
    expect(clearQueuedFollowUps()).toEqual([]);
  });

  it('puts the first queued line back in the draft on stop, and keeps the rest queued', () => {
    expect(
      restoreQueuedFollowUpOnCancel({
        draft: '',
        queue: [
          { id: 'q-2', text: '第二条' },
          { id: 'q-3', text: '第三条' },
        ],
      }),
    ).toEqual({
      draft: '第二条',
      queue: [{ id: 'q-3', text: '第三条' }],
    });
    expect(
      restoreQueuedFollowUpOnCancel({
        draft: '还在写',
        queue: [
          { id: 'q-2', text: '第二条' },
          { id: 'q-3', text: '第三条' },
        ],
      }),
    ).toEqual({
      draft: '还在写',
      queue: [
        { id: 'q-2', text: '第二条' },
        { id: 'q-3', text: '第三条' },
      ],
    });
    expect(restoreQueuedFollowUpOnCancel({ draft: '', queue: [] })).toEqual({
      draft: '',
      queue: [],
    });
  });
});

describe('grok follow-up after the current turn', () => {
  it('queues only for a generating Grok / Kiro / Claude continuous session', () => {
    expect(
      grokCanQueueFollowUp({
        agentId: 'grok',
        runtimeEnabled: true,
        phase: 'running',
        sending: true,
      }),
    ).toBe(true);
    expect(
      grokCanQueueFollowUp({
        agentId: 'codex',
        runtimeEnabled: true,
        phase: 'running',
        sending: true,
      }),
    ).toBe(false);
    expect(
      grokCanQueueFollowUp({
        agentId: 'grok',
        runtimeEnabled: false,
        phase: 'running',
        sending: true,
      }),
    ).toBe(false);
    expect(
      grokCanQueueFollowUp({
        agentId: 'grok',
        runtimeEnabled: true,
        phase: 'idle',
        sending: false,
      }),
    ).toBe(false);
    expect(
      grokCanQueueFollowUp({
        agentId: 'kiro',
        runtimeEnabled: true,
        phase: 'running',
        sending: true,
      }),
    ).toBe(true);
    expect(
      grokCanQueueFollowUp({
        agentId: 'claude',
        runtimeEnabled: true,
        phase: 'running',
        sending: true,
      }),
    ).toBe(true);
  });

  it('flushes only when a live turn completes', () => {
    expect(grokShouldFlushFollowUp('running', 'completed')).toBe(true);
    expect(grokShouldFlushFollowUp('waiting', 'completed')).toBe(true);
    expect(grokShouldFlushFollowUp('running', 'cancelled')).toBe(false);
    expect(grokShouldFlushFollowUp('running', 'failed')).toBe(false);
    expect(grokShouldFlushFollowUp('idle', 'completed')).toBe(false);
  });

  it('offers continue only for old Grok chats that still have a session', () => {
    expect(
      grokLegacyContinueKind({
        agentId: 'grok',
        runtimeEnabled: false,
        runtimeReady: true,
        hasMessages: true,
        nativeSessionId: 'sess-1',
      }),
    ).toBe('continue');
    expect(
      grokLegacyContinueKind({
        agentId: 'grok',
        runtimeEnabled: false,
        runtimeReady: true,
        hasMessages: true,
        nativeSessionId: null,
      }),
    ).toBe('newChat');
    expect(
      grokLegacyContinueKind({
        agentId: 'grok',
        runtimeEnabled: true,
        runtimeReady: true,
        hasMessages: true,
        nativeSessionId: 'sess-1',
      }),
    ).toBe(null);
    expect(
      grokLegacyContinueKind({
        agentId: 'codex',
        runtimeEnabled: false,
        runtimeReady: true,
        hasMessages: true,
        nativeSessionId: 'thread-1',
      }),
    ).toBe(null);
    expect(
      grokLegacyContinueKind({
        agentId: 'kiro',
        runtimeEnabled: false,
        runtimeReady: true,
        hasMessages: true,
        nativeSessionId: 'sess-kiro',
      }),
    ).toBe('newChat');
    expect(
      grokLegacyContinueKind({
        agentId: 'claude',
        runtimeEnabled: false,
        runtimeReady: true,
        hasMessages: true,
        nativeSessionId: 'sess-claude',
      }),
    ).toBe(null);
  });

  it('does not treat a missing snapshot as an old chat', () => {
    expect(
      grokLegacyContinueKind({
        agentId: 'grok',
        runtimeEnabled: false,
        runtimeReady: false,
        hasMessages: true,
        nativeSessionId: null,
      }),
    ).toBeNull();
    expect(
      grokLegacyContinueKind({
        agentId: 'kiro',
        runtimeEnabled: false,
        runtimeReady: false,
        hasMessages: true,
        nativeSessionId: 'sess-kiro',
      }),
    ).toBeNull();
  });

  it('keeps Kiro HTTP history out of the ACP continuation flow', () => {
    expect(grokLegacyContinueKind({
      agentId: 'kiro',
      runtimeEnabled: false,
      runtimeReady: true,
      hasMessages: true,
      nativeSessionId: 'kiro-http:conversation-1',
    })).toBe('newChat');
    expect(grokLegacyContinueKind({
      agentId: 'kiro',
      runtimeEnabled: true,
      runtimeReady: true,
      hasMessages: true,
      nativeSessionId: 'session-1',
    })).toBeNull();
  });
});
