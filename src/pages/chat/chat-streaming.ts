import type { AgentProcessView } from '@/lib/chat-process';
import type { MessageKey } from '@/lib/i18n';

/**
 * Active Codex / Grok / Kiro snapshot poll. The page shows the durable
 * `currentMessage` from that read — not a client-side token drip.
 */
export const RUNTIME_SNAPSHOT_POLL_ACTIVE_MS = 80;

/** Background conversations keep the slower cadence so the focused turn can poll more often. */
export const RUNTIME_SNAPSHOT_POLL_BACKGROUND_MS = 400;

export type StreamingActivity = 'thinking' | 'writing';

export function streamingActivity(
  process?: AgentProcessView | null,
  hasContent = false,
): StreamingActivity {
  if (hasContent) return 'writing';
  if (!process) return 'thinking';
  for (let i = process.steps.length - 1; i >= 0; i -= 1) {
    const step = process.steps[i];
    if (step.type === 'thinking') return step.done ? 'writing' : 'thinking';
  }
  return 'thinking';
}

export function streamingPlaceholderKey(
  process?: AgentProcessView | null,
): MessageKey {
  return streamingActivity(process, false) === 'thinking'
    ? 'chat.bubble.thinking'
    : 'chat.bubble.writing';
}

export function streamingStatusKey(
  process?: AgentProcessView | null,
  hasContent = false,
): MessageKey {
  return streamingActivity(process, hasContent) === 'thinking'
    ? 'chat.status.thinking'
    : 'chat.status.writing';
}
