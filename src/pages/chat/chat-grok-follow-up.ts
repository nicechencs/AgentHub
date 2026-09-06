import { isRuntimeActive } from './chat-runtime-model';

/** Grok has no mid-turn inject. Queue only while a continuous session is generating. */
export function grokCanQueueFollowUp(input: {
  agentId?: string | null;
  runtimeEnabled?: boolean;
  phase?: string | null;
  sending: boolean;
}): boolean {
  if (input.agentId !== 'grok' || !input.runtimeEnabled || !input.sending) return false;
  return isRuntimeActive(input.phase ?? 'idle');
}

/** Drain the queued line only after a successful turn. Stop/fail keep it unsent. */
/** Old Grok chats stay on print send until the user explicitly continues. */
export function grokLegacyContinueKind(input: {
  agentId?: string | null;
  runtimeEnabled?: boolean;
  hasMessages: boolean;
  nativeSessionId?: string | null;
}): 'continue' | 'newChat' | null {
  if (input.agentId !== 'grok' || input.runtimeEnabled || !input.hasMessages) return null;
  return input.nativeSessionId?.trim() ? 'continue' : 'newChat';
}

export function grokShouldFlushFollowUp(
  previousPhase: string | null | undefined,
  nextPhase: string,
): boolean {
  if (!previousPhase || !isRuntimeActive(previousPhase)) return false;
  return nextPhase === 'completed';
}
