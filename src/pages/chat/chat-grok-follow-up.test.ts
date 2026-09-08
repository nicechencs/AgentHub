import { describe, expect, it } from 'vitest';
import {
  appendQueuedFollowUp,
  chatBusySendMode,
  grokCanQueueFollowUp,
  grokLegacyContinueKind,
  grokShouldFlushFollowUp,
  prependQueuedFollowUp,
  queuedFollowUpLabel,
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

  it('keeps second and third follow-ups in order', () => {
    const queued = appendQueuedFollowUp(appendQueuedFollowUp([], '第二条'), '第三条');
    expect(queuedFollowUpLabel(queued)).toBe('第二条；第三条');
    const first = shiftQueuedFollowUp(queued);
    expect(first).toEqual({ next: '第二条', rest: ['第三条'] });
    expect(shiftQueuedFollowUp(first?.rest ?? [])).toEqual({ next: '第三条', rest: [] });
    expect(queuedFollowUpLabel(prependQueuedFollowUp(['第三条'], '第二条'))).toBe('第二条；第三条');
    expect(appendQueuedFollowUp(['已排队'], '  ')).toEqual(['已排队']);
    expect(queuedFollowUpLabel([])).toBeNull();
    expect(shiftQueuedFollowUp([])).toBeNull();
  });

  it('puts the first queued line back in the draft on stop, and keeps the rest queued', () => {
    expect(
      restoreQueuedFollowUpOnCancel({
        draft: '',
        queue: ['第二条', '第三条'],
      }),
    ).toEqual({ draft: '第二条', queue: ['第三条'] });
    expect(
      restoreQueuedFollowUpOnCancel({
        draft: '还在写',
        queue: ['第二条', '第三条'],
      }),
    ).toEqual({ draft: '还在写', queue: ['第二条', '第三条'] });
    expect(restoreQueuedFollowUpOnCancel({ draft: '', queue: [] })).toEqual({
      draft: '',
      queue: [],
    });
  });
});

describe('grok follow-up after the current turn', () => {
  it('queues only for a generating Grok continuous session', () => {
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
        hasMessages: true,
        nativeSessionId: 'sess-1',
      }),
    ).toBe('continue');
    expect(
      grokLegacyContinueKind({
        agentId: 'grok',
        runtimeEnabled: false,
        hasMessages: true,
        nativeSessionId: null,
      }),
    ).toBe('newChat');
    expect(
      grokLegacyContinueKind({
        agentId: 'grok',
        runtimeEnabled: true,
        hasMessages: true,
        nativeSessionId: 'sess-1',
      }),
    ).toBe(null);
    expect(
      grokLegacyContinueKind({
        agentId: 'codex',
        runtimeEnabled: false,
        hasMessages: true,
        nativeSessionId: 'thread-1',
      }),
    ).toBe(null);
    expect(
      grokLegacyContinueKind({
        agentId: 'kiro',
        runtimeEnabled: false,
        hasMessages: true,
        nativeSessionId: 'sess-kiro',
      }),
    ).toBe('newChat');
  });

  it('keeps Kiro HTTP history out of the ACP continuation flow', () => {
    expect(grokLegacyContinueKind({
      agentId: 'kiro',
      runtimeEnabled: false,
      hasMessages: true,
      nativeSessionId: 'kiro-http:conversation-1',
    })).toBe('newChat');
    expect(grokLegacyContinueKind({
      agentId: 'kiro',
      runtimeEnabled: true,
      hasMessages: true,
      nativeSessionId: 'session-1',
    })).toBeNull();
  });
});
