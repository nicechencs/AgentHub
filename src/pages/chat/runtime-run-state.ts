import type { RuntimePhase, RuntimeSnapshot } from '@/lib/backend/contracts/chat-runtime';
import type { ChatEvent, ChatMessage } from '@/lib/types';
import { processKey, reduceProcessEvent, type ProcessMap } from '@/lib/chat-process';
import { isRuntimeActive } from './chat-runtime-model';

export interface RuntimeRunRecord {
  enabled: boolean;
  runId: string | null;
  phase: RuntimePhase;
  pendingStart: boolean;
  cancelRequested: boolean;
  lastSequence: number;
}

export interface RuntimeConversationView {
  processMap: ProcessMap;
  streams: Record<string, string>;
}

export interface RuntimeSnapshotVersion {
  sourceVersion: number;
  lastSequence: number;
  phase: RuntimePhase;
  runId: string | null;
}

export type RuntimeEventMode = 'runtime' | 'legacy';

export type RuntimeCancelTarget =
  | { kind: 'pending' }
  | { kind: 'runtime'; runId: string }
  | { kind: 'legacy' }
  | { kind: 'none' };

/** Keep the latest durable state for every conversation, including inactive ones. */
export function rememberRuntimeSnapshot(
  records: Map<string, RuntimeRunRecord>,
  snapshot: RuntimeSnapshot,
): RuntimeRunRecord {
  const previous = records.get(snapshot.conversationId);
  const next: RuntimeRunRecord = {
    enabled: snapshot.enabled,
    runId: snapshot.runId,
    phase: snapshot.phase,
    pendingStart: false,
    cancelRequested: isRuntimeActive(snapshot.phase) && Boolean(previous?.cancelRequested),
    lastSequence: snapshot.lastSequence,
  };
  records.set(snapshot.conversationId, next);
  return next;
}

export function beginRuntimeStart(
  records: Map<string, RuntimeRunRecord>,
  conversationId: string,
  lastSequence: number,
): RuntimeRunRecord {
  const next: RuntimeRunRecord = {
    enabled: true,
    runId: null,
    phase: 'starting',
    pendingStart: true,
    cancelRequested: false,
    lastSequence,
  };
  records.set(conversationId, next);
  return next;
}

/** Resolve cancellation against the target conversation, rather than the selected one. */
export function requestRuntimeCancel(
  records: Map<string, RuntimeRunRecord>,
  conversationId: string,
): RuntimeCancelTarget {
  const record = records.get(conversationId);
  if (!record || !record.enabled) return { kind: 'legacy' };
  if (record.pendingStart) {
    record.cancelRequested = true;
    return { kind: 'pending' };
  }
  if (!record.runId) return { kind: 'none' };
  if (!isRuntimeActive(record.phase)) return { kind: 'none' };
  record.cancelRequested = true;
  return { kind: 'runtime', runId: record.runId };
}

export function isRuntimeTerminal(record: RuntimeRunRecord | undefined): boolean {
  return Boolean(record && !isRuntimeActive(record.phase));
}

/**
 * Accept only a newer snapshot source whose durable sequence cannot move
 * state backwards. Source versions are allocated by the per-conversation
 * async lane in the hook, while the sequence check protects against a stale
 * server response that was already in flight before a newer poll.
 */
export function acceptRuntimeSnapshotVersion(
  versions: Map<string, RuntimeSnapshotVersion>,
  snapshot: RuntimeSnapshot,
  sourceVersion: number,
): boolean {
  const previous = versions.get(snapshot.conversationId);
  if (previous) {
    if (sourceVersion < previous.sourceVersion || snapshot.lastSequence < previous.lastSequence) {
      return false;
    }
    if (snapshot.lastSequence === previous.lastSequence) {
      if (previous.runId && !snapshot.runId) return false;
      if (
        !isRuntimeActive(previous.phase) &&
        isRuntimeActive(snapshot.phase) &&
        snapshot.runId === previous.runId
      ) return false;
      if (previous.phase === 'cancelling' && isRuntimeActive(snapshot.phase) && snapshot.phase !== 'cancelling') {
        return false;
      }
    }
  }
  versions.set(snapshot.conversationId, {
    sourceVersion,
    lastSequence: snapshot.lastSequence,
    phase: snapshot.phase,
    runId: snapshot.runId,
  });
  return true;
}

export function isLatestRuntimeSnapshot(
  versions: Map<string, RuntimeSnapshotVersion>,
  conversationId: string,
  sourceVersion: number,
): boolean {
  return versions.get(conversationId)?.sourceVersion === sourceVersion;
}

export function advanceRuntimeWatermark(
  watermarks: Map<string, number>,
  conversationId: string,
  sequence: number,
): number {
  const next = Math.max(watermarks.get(conversationId) ?? 0, sequence);
  watermarks.set(conversationId, next);
  return next;
}

/** Serialize every snapshot-producing operation for one conversation. */
export function enqueueRuntimeSnapshot(
  lanes: Map<string, Promise<void>>,
  sourceVersions: Map<string, number>,
  conversationId: string,
  source: () => Promise<RuntimeSnapshot>,
  handle: (snapshot: RuntimeSnapshot, sourceVersion: number) => void | Promise<void>,
): Promise<RuntimeSnapshot> {
  const sourceVersion = (sourceVersions.get(conversationId) ?? 0) + 1;
  sourceVersions.set(conversationId, sourceVersion);
  const previous = lanes.get(conversationId) ?? Promise.resolve();
  const next = previous
    .catch(() => {})
    .then(async () => {
      const snapshot = await source();
      await handle(snapshot, sourceVersion);
      return snapshot;
    });
  lanes.set(conversationId, next.then(() => undefined, () => undefined));
  return next;
}

export function runtimeConversationView(
  views: Map<string, RuntimeConversationView>,
  conversationId: string,
): RuntimeConversationView {
  const existing = views.get(conversationId);
  if (existing) return existing;
  const created: RuntimeConversationView = { processMap: {}, streams: {} };
  views.set(conversationId, created);
  return created;
}

/** Upsert the durable runtime message without duplicating a local optimistic row. */
export function upsertRuntimeMessage(
  messages: readonly ChatMessage[],
  message: ChatMessage,
): ChatMessage[] {
  const byId = messages.findIndex((item) => item.id === message.id);
  if (byId >= 0) return messages.map((item, index) => (index === byId ? message : item));

  const byTurnAgent =
    message.role === 'agent'
      ? messages.findIndex(
          (item) =>
            item.role === 'agent' &&
            item.turn === message.turn &&
            item.agentId === message.agentId,
        )
      : -1;
  if (byTurnAgent < 0) return [...messages, message];

  return messages.map((item, index) => (index === byTurnAgent ? message : item));
}

/** Apply runtime events to a conversation-owned process/stream view. */
export function reduceRuntimeConversationEvent(
  views: Map<string, RuntimeConversationView>,
  conversationId: string,
  event: ChatEvent,
  messages: readonly ChatMessage[] = [],
  mode: RuntimeEventMode = 'legacy',
): RuntimeConversationView {
  const previous = runtimeConversationView(views, conversationId);
  const streams = { ...previous.streams };
  if (mode === 'legacy' && event.type === 'started') {
    for (const agent of event.agents) streams[processKey(event.turn, agent)] = '';
  } else if (mode === 'legacy' && event.type === 'agentChunk' && event.stream === 'stdout') {
    const key = processKey(event.turn, event.agent);
    const existing = messages.find(
      (message) =>
        message.role === 'agent' &&
        message.agentId === event.agent &&
        message.turn === event.turn &&
        message.status === 'running',
    );
    const previousText = streams[key];
    const base = previousText || existing?.content || '';
    streams[key] = `${base}${event.text}`;
  } else if (mode === 'legacy' && event.type === 'agentFinished') {
    const key = processKey(event.turn, event.agent);
    streams[key] = event.message.content || streams[key] || '';
  }
  const next: RuntimeConversationView = {
    processMap: reduceProcessEvent(previous.processMap, event),
    streams,
  };
  views.set(conversationId, next);
  return next;
}
