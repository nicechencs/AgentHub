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

/**
 * Codex empty chats advertise `enabled` so the first send uses the runtime
 * path — that must not lock Agent / cwd. Lock only after the continuous
 * session has actually started (or a native thread is already attached).
 */
export function isRuntimeSessionLocked(
  runtime: Pick<RuntimeSnapshot, 'enabled' | 'phase' | 'runId'> | null | undefined,
  extras?: { nativeSessionId?: string | null },
): boolean {
  if (!runtime?.enabled) return false;
  if (extras?.nativeSessionId) return true;
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
