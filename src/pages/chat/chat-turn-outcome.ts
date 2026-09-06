import type { ChatMessage, ChatMessageStatus } from '@/lib/types';
import type { TurnGroup } from './chat-format';

export type ChatTurnOutcomeKind = 'failed' | 'interrupted' | 'cancelled' | 'timeout';

export interface ChatTurnOutcome {
  kind: ChatTurnOutcomeKind;
  prompt: string;
  errorText: string | null;
  /** Outcome is derived only from message.status / structured error — never from assistant prose. */
  source: 'message-status';
}

const OUTCOME_STATUSES = new Set<ChatMessageStatus>(['failed', 'cancelled', 'timeout']);

function looksInterrupted(message: ChatMessage): boolean {
  // Only structured error text — never assistant prose.
  const hay = (message.error ?? '').toLowerCase();
  return (
    hay.includes('runtime interrupted') ||
    hay.includes('chat.runtime.interrupted') ||
    hay.includes('codex process stopped') ||
    hay.includes('thread is unavailable')
  );
}

/**
 * Last-turn recovery banner model.
 * Uses durable message status only; model prose claiming "tests passed" never upgrades outcome.
 */
export function lastTurnOutcome(turns: TurnGroup[], sending: boolean): ChatTurnOutcome | null {
  if (sending || turns.length === 0) return null;
  const last = turns[turns.length - 1];
  const prompt = last.user?.content?.trim() ?? '';
  if (!prompt) return null;
  const failedAgents = last.agents.filter((m) => OUTCOME_STATUSES.has(m.status));
  if (failedAgents.length === 0) return null;

  const primary = failedAgents[failedAgents.length - 1];
  if (primary.status === 'timeout') {
    return {
      kind: 'timeout',
      prompt,
      errorText: primary.error?.trim() || null,
      source: 'message-status',
    };
  }
  if (primary.status === 'cancelled') {
    return {
      kind: looksInterrupted(primary) ? 'interrupted' : 'cancelled',
      prompt,
      errorText: primary.error?.trim() || null,
      source: 'message-status',
    };
  }
  return {
    kind: 'failed',
    prompt,
    errorText: primary.error?.trim() || null,
    source: 'message-status',
  };
}
