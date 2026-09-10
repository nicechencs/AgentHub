import type { RuntimePhase } from '@/lib/backend/contracts/chat-runtime';
import { isRuntimeActive } from './chat-runtime-model';

export type QueuedFollowUpItem = {
  id: string;
  text: string;
};

let queuedFollowUpSeq = 0;

function isQueueFollowUpAgent(agentId?: string | null): boolean {
  return agentId === 'grok' || agentId === 'kiro' || agentId === 'claude';
}

function isAcpLegacyContinueAgent(agentId?: string | null): boolean {
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

export function createQueuedFollowUpItem(text: string, id?: string): QueuedFollowUpItem | null {
  const next = text.trim();
  if (!next) return null;
  queuedFollowUpSeq += 1;
  return { id: id ?? `queued-${queuedFollowUpSeq}`, text: next };
}

export function queuedFollowUpItems(queue: readonly QueuedFollowUpItem[]): QueuedFollowUpItem[] {
  return queue.filter((item) => item.text.trim());
}

export function appendQueuedFollowUp(
  queue: readonly QueuedFollowUpItem[],
  prompt: string,
  id?: string,
): QueuedFollowUpItem[] {
  const item = createQueuedFollowUpItem(prompt, id);
  if (!item) return [...queue];
  return [...queue, item];
}

export function prependQueuedFollowUp(
  queue: readonly QueuedFollowUpItem[],
  prompt: string,
  id?: string,
): QueuedFollowUpItem[] {
  const item = createQueuedFollowUpItem(prompt, id);
  if (!item) return [...queue];
  return [item, ...queue];
}

export function removeQueuedFollowUp(
  queue: readonly QueuedFollowUpItem[],
  id: string,
): QueuedFollowUpItem[] {
  return queue.filter((item) => item.id !== id);
}

export function clearQueuedFollowUps(): QueuedFollowUpItem[] {
  return [];
}

export function shiftQueuedFollowUp(
  queue: readonly QueuedFollowUpItem[],
): { next: QueuedFollowUpItem; rest: QueuedFollowUpItem[] } | null {
  const items = queuedFollowUpItems(queue);
  if (items.length === 0) return null;
  const [next, ...rest] = items;
  return { next, rest };
}

export function queuedFollowUpCount(queue: readonly QueuedFollowUpItem[]): number {
  return queuedFollowUpItems(queue).length;
}

export function restoreQueuedFollowUpOnCancel(input: {
  draft: string;
  queue: readonly QueuedFollowUpItem[];
}): { draft: string; queue: QueuedFollowUpItem[] } {
  const queue = queuedFollowUpItems(input.queue);
  if (queue.length === 0) return { draft: input.draft, queue: [] };
  if (input.draft.trim()) return { draft: input.draft, queue: [...queue] };
  return { draft: queue[0].text, queue: queue.slice(1) };
}

/** Grok / Kiro / Claude have no mid-turn inject. Queue only while a continuous session is generating. */
export function grokCanQueueFollowUp(input: {
  agentId?: string | null;
  runtimeEnabled?: boolean;
  phase?: RuntimePhase | null;
  sending: boolean;
}): boolean {
  if (!isQueueFollowUpAgent(input.agentId) || !input.runtimeEnabled || !input.sending) return false;
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
  if (!isAcpLegacyContinueAgent(input.agentId) || input.runtimeEnabled || !input.hasMessages) return null;
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
