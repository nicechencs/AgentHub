import type { RuntimePhase } from '@/lib/backend/contracts/chat-runtime';
import { isRuntimeActive } from './chat-runtime-model';

function isAcpFollowUpAgent(agentId?: string | null): boolean {
  return agentId === 'grok' || agentId === 'kiro';
}

export type ChatBusySendMode = 'steer' | 'queue';

/** A line already waiting for this turn must not be overtaken by a later inject. */
export function chatBusySendMode(input: {
  sending: boolean;
  runtimeEnabled?: boolean;
  steer?: boolean;
  runId?: string | null;
  phase?: RuntimePhase | null;
  queued?: boolean;
}): ChatBusySendMode | null {
  if (!input.sending) return null;
  if (input.queued) return 'queue';
  if (
    input.runtimeEnabled
    && input.steer === true
    && Boolean(input.runId?.trim())
    && isRuntimeActive(input.phase ?? 'idle')
  ) {
    return 'steer';
  }
  return 'queue';
}

export function appendQueuedFollowUp(queue: readonly string[], prompt: string): string[] {
  const next = prompt.trim();
  if (!next) return [...queue];
  return [...queue, next];
}

export function prependQueuedFollowUp(queue: readonly string[], prompt: string): string[] {
  const next = prompt.trim();
  if (!next) return [...queue];
  return [next, ...queue];
}

export function shiftQueuedFollowUp(
  queue: readonly string[],
): { next: string; rest: string[] } | null {
  if (queue.length === 0) return null;
  const [next, ...rest] = queue;
  return { next, rest };
}

export function queuedFollowUpLabel(queue: readonly string[]): string | null {
  const items = queue.map((item) => item.trim()).filter(Boolean);
  if (items.length === 0) return null;
  return items.join('；');
}

export function restoreQueuedFollowUpOnCancel(input: {
  draft: string;
  queue: readonly string[];
}): { draft: string; queue: string[] } {
  if (input.queue.length === 0) return { draft: input.draft, queue: [] };
  if (input.draft.trim()) return { draft: input.draft, queue: [...input.queue] };
  return { draft: input.queue[0], queue: input.queue.slice(1) };
}

/** Grok/Kiro have no mid-turn inject. Queue only while a continuous session is generating. */
export function grokCanQueueFollowUp(input: {
  agentId?: string | null;
  runtimeEnabled?: boolean;
  phase?: RuntimePhase | null;
  sending: boolean;
}): boolean {
  if (!isAcpFollowUpAgent(input.agentId) || !input.runtimeEnabled || !input.sending) return false;
  return isRuntimeActive(input.phase ?? 'idle');
}

/** Only Grok can reconnect an old native session in a fresh ACP process. */
export function grokLegacyContinueKind(input: {
  agentId?: string | null;
  runtimeEnabled?: boolean;
  /** Snapshot has arrived for this conversation. Unknown must not look like legacy. */
  runtimeReady: boolean;
  hasMessages: boolean;
  nativeSessionId?: string | null;
}): 'continue' | 'newChat' | null {
  if (!input.runtimeReady) return null;
  if (!isAcpFollowUpAgent(input.agentId) || input.runtimeEnabled || !input.hasMessages) return null;
  // Kiro session ids belong to the original ACP process. HTTP ids also cannot
  // be loaded by the CLI; keep old history without offering a lossy upgrade.
  if (input.agentId === 'kiro') return 'newChat';
  return input.nativeSessionId?.trim() ? 'continue' : 'newChat';
}

/** Drain the queued line only after a successful turn. Stop/fail keep it unsent. */
export function grokShouldFlushFollowUp(
  previousPhase: RuntimePhase | null | undefined,
  nextPhase: RuntimePhase,
): boolean {
  if (!previousPhase || !isRuntimeActive(previousPhase)) return false;
  return nextPhase === 'completed';
}
