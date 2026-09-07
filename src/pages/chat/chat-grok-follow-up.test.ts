import { describe, expect, it } from 'vitest';
import {
  grokCanQueueFollowUp,
  grokLegacyContinueKind,
  grokShouldFlushFollowUp,
} from './chat-grok-follow-up';

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
