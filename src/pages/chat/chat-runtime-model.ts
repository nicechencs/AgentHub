import type { RuntimeRequest, RuntimeSnapshot } from '@/lib/api/chat';

export type RuntimeTransport =
  | { kind: 'runtime'; snapshot: RuntimeSnapshot }
  | { kind: 'legacy'; snapshot: RuntimeSnapshot }
  | { kind: 'unavailable' };

/** A failed runtime read is never permission to start the legacy CLI path. */
export async function readRuntimeTransport(
  read: () => Promise<RuntimeSnapshot>,
): Promise<RuntimeTransport> {
  try {
    const snapshot = await read();
    return snapshot.enabled ? { kind: 'runtime', snapshot } : { kind: 'legacy', snapshot };
  } catch {
    return { kind: 'unavailable' };
  }
}

export function isRuntimeActive(phase: RuntimeSnapshot['phase']): boolean {
  return ['starting', 'running', 'waiting', 'cancelling'].includes(phase);
}

export type RuntimeSessionLockExtras = {
  conversationId?: string | null;
  nativeSessionId?: string | null;
  hasMessages?: boolean;
};

/**
 * Empty chats must keep Agent / cwd editable — including Codex rows that
 * advertise `enabled` so the first send uses the runtime path, and a leftover
 * snapshot from the previous conversation. Lock only after *this* conversation
 * has a message, a native thread, or a started runtime session.
 */
export function isRuntimeSessionLocked(
  runtime: Pick<RuntimeSnapshot, 'enabled' | 'phase' | 'runId' | 'conversationId'> | null | undefined,
  extras?: RuntimeSessionLockExtras,
): boolean {
  if (extras?.hasMessages) return true;
  if (extras?.nativeSessionId?.trim()) return true;
  if (!runtime?.enabled) return false;
  if (extras?.conversationId && runtime.conversationId !== extras.conversationId) {
    return false;
  }
  return runtime.phase !== 'idle' || Boolean(runtime.runId);
}

/** A late poll for A must not alter the second visit to A after A → B → A. */
export function acceptsRuntimeSnapshot(
  activeId: string | null,
  activeGeneration: number,
  conversationId: string,
  snapshotGeneration: number,
): boolean {
  return activeId === conversationId && activeGeneration === snapshotGeneration;
}

export function requestMatchesRuntime(request: RuntimeRequest, activeRunId: string | null): boolean {
  return request.runId === activeRunId;
}

export function isLatestRuntimeRead(readId: number, newestReadId: number): boolean {
  return readId === newestReadId;
}

export function canSubmitRuntimeQuestions(
  request: Pick<RuntimeRequest, 'kind' | 'questions'>,
  answers: Record<string, string[]>,
): boolean {
  return request.kind !== 'question' || request.questions.every((question) => Boolean(answers[question.id]?.length));
}
