import { describe, expect, it } from 'vitest';
import type { RuntimeSnapshot } from '@/lib/api/chat';
import {
  beginRuntimeStart,
  acceptRuntimeSnapshotVersion,
  advanceRuntimeWatermark,
  enqueueRuntimeSnapshot,
  isLatestRuntimeSnapshot,
  isRuntimeTerminal,
  rememberRuntimeSnapshot,
  reduceRuntimeConversationEvent,
  requestRuntimeCancel,
  upsertRuntimeMessage,
  type RuntimeSnapshotVersion,
  type RuntimeRunRecord,
} from './runtime-run-state';

const snapshot = (conversationId: string, phase: RuntimeSnapshot['phase'], runId: string | null): RuntimeSnapshot => ({
  conversationId,
  enabled: true,
  runId,
  phase,
  lastSequence: 3,
  events: [],
  pendingRequests: [],
  gap: false,
});

describe('runtime run ownership', () => {
  it('cancels an inactive conversation with its own run id', () => {
    const records = new Map<string, RuntimeRunRecord>();
    rememberRuntimeSnapshot(records, snapshot('a', 'running', 'run-a'));
    rememberRuntimeSnapshot(records, snapshot('b', 'running', 'run-b'));

    expect(requestRuntimeCancel(records, 'a')).toEqual({ kind: 'runtime', runId: 'run-a' });
    expect(requestRuntimeCancel(records, 'b')).toEqual({ kind: 'runtime', runId: 'run-b' });
  });

  it('records a cancel intent while start has no run id yet', () => {
    const records = new Map<string, RuntimeRunRecord>();
    beginRuntimeStart(records, 'a', 0);

    expect(requestRuntimeCancel(records, 'a')).toEqual({ kind: 'pending' });
    expect(records.get('a')?.cancelRequested).toBe(true);
  });

  it('does not route a terminal runtime to legacy cancel', () => {
    const records = new Map<string, RuntimeRunRecord>();
    rememberRuntimeSnapshot(records, snapshot('a', 'completed', 'run-a'));

    expect(requestRuntimeCancel(records, 'a')).toEqual({ kind: 'none' });
    expect(isRuntimeTerminal(records.get('a'))).toBe(true);
  });

  it('does not mark an idle runtime without a run as pending', () => {
    const records = new Map<string, RuntimeRunRecord>();
    rememberRuntimeSnapshot(records, snapshot('a', 'idle', null));

    expect(requestRuntimeCancel(records, 'a')).toEqual({ kind: 'none' });
  });

  it('keeps legacy cancellation for conversations without runtime state', () => {
    expect(requestRuntimeCancel(new Map(), 'legacy')).toEqual({ kind: 'legacy' });
  });
});

describe('runtime state across visits and async sources', () => {
  it('keeps process output and appends a later chunk to the persisted body', () => {
    const views = new Map();
    reduceRuntimeConversationEvent(views, 'a', { type: 'started', turn: 1, agents: ['codex'] });
    reduceRuntimeConversationEvent(views, 'a', {
      type: 'agentChunk', turn: 1, agent: 'codex', stream: 'stdout', text: 'hello',
    });

    const next = reduceRuntimeConversationEvent(views, 'a', {
      type: 'agentChunk', turn: 1, agent: 'codex', stream: 'stdout', text: ' world',
    });

    expect(next.streams['1:codex']).toBe('hello world');
    expect(next.processMap['1:codex'].stdout).toBe('hello world');

    const legacyReplay = reduceRuntimeConversationEvent(new Map(), 'a', {
      type: 'agentChunk', turn: 1, agent: 'codex', stream: 'stdout', text: 'hello',
    }, [{
      id: 'agent-1', conversationId: 'a', turn: 1, role: 'agent', agentId: 'codex',
      content: 'hello', status: 'running', durationMs: 0, createdAt: '',
    }]);
    expect(legacyReplay.streams['1:codex']).toBe('hellohello');

    const prefixReplay = reduceRuntimeConversationEvent(new Map(), 'a', {
      type: 'agentChunk', turn: 1, agent: 'codex', stream: 'stdout', text: 'abc',
    }, [{
      id: 'agent-1', conversationId: 'a', turn: 1, role: 'agent', agentId: 'codex',
      content: 'a', status: 'running', durationMs: 0, createdAt: '',
    }]);
    expect(prefixReplay.streams['1:codex']).toBe('aabc');

    const runtimeView = reduceRuntimeConversationEvent(new Map(), 'a', {
      type: 'agentChunk', turn: 1, agent: 'codex', stream: 'stdout', text: 'ha',
    }, [], 'runtime');
    expect(runtimeView.streams['1:codex']).toBeUndefined();
    expect(runtimeView.processMap['1:codex'].stdout).toBe('ha');

    let messages = upsertRuntimeMessage([], {
      id: 'runtime-1', conversationId: 'a', turn: 1, role: 'agent', agentId: 'codex',
      content: 'ha', status: 'running', durationMs: 0, createdAt: '',
    });
    messages = upsertRuntimeMessage(messages, {
      id: 'runtime-2', conversationId: 'a', turn: 1, role: 'agent', agentId: 'codex',
      content: 'haha', status: 'running', durationMs: 0, createdAt: '',
    });
    expect(messages).toHaveLength(1);
    expect(messages[0].content).toBe('haha');
  });

  it('serializes a start response ahead of a later poll and rejects an older rewind', async () => {
    const lanes = new Map<string, Promise<void>>();
    const versions = new Map<string, number>();
    const accepted = new Map<string, RuntimeSnapshotVersion>();
    const applied: string[] = [];
    let releaseFirst: ((snapshot: RuntimeSnapshot) => void) | undefined;
    const first = new Promise<RuntimeSnapshot>((resolve) => { releaseFirst = resolve; });
    const firstRequest = enqueueRuntimeSnapshot(
      lanes, versions, 'a', () => first,
      (snapshot, sourceVersion) => {
        if (acceptRuntimeSnapshotVersion(accepted, snapshot, sourceVersion)) applied.push(snapshot.phase);
      },
    );
    let secondStarted = false;
    const secondRequest = enqueueRuntimeSnapshot(
      lanes, versions, 'a', async () => {
        secondStarted = true;
        return snapshot('a', 'completed', 'run-a');
      },
      (snapshot, sourceVersion) => {
        if (acceptRuntimeSnapshotVersion(accepted, snapshot, sourceVersion)) applied.push(snapshot.phase);
      },
    );

    expect(secondStarted).toBe(false);
    releaseFirst!(snapshot('a', 'running', 'run-a'));
    await firstRequest;
    await secondRequest;
    expect(applied).toEqual(['running', 'completed']);
    expect(secondStarted).toBe(true);
    expect(acceptRuntimeSnapshotVersion(
      accepted,
      snapshot('a', 'running', 'run-a'),
      1,
    )).toBe(false);
    expect(acceptRuntimeSnapshotVersion(
      accepted,
      snapshot('a', 'running', 'run-b'),
      3,
    )).toBe(true);
    expect(isLatestRuntimeSnapshot(accepted, 'a', 3)).toBe(true);
    expect(isLatestRuntimeSnapshot(accepted, 'a', 1)).toBe(false);
  });

  it('keeps the event watermark after an inactive replay early return', () => {
    const watermarks = new Map<string, number>();
    expect(advanceRuntimeWatermark(watermarks, 'a', 4)).toBe(4);
    expect(advanceRuntimeWatermark(watermarks, 'a', 2)).toBe(4);
    expect(watermarks.get('a')).toBe(4);
  });
});
