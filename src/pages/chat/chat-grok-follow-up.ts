import type { RuntimePhase } from '@/lib/backend/contracts/chat-runtime';
import { isRuntimeActive } from './chat-runtime-model';

function isAcpFollowUpAgent(agentId?: string | null): boolean {
  return agentId === 'grok' || agentId === 'kiro';
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

/** Drain the queued line only after a successful turn. Stop/fail keep it unsent. */
/** Only Grok can reconnect an old native session in a fresh ACP process. */
export function grokLegacyContinueKind(input: {
  agentId?: string | null;
  runtimeEnabled?: boolean;
  hasMessages: boolean;
  nativeSessionId?: string | null;
}): 'continue' | 'newChat' | null {
  if (!isAcpFollowUpAgent(input.agentId) || input.runtimeEnabled || !input.hasMessages) return null;
  // Kiro session ids belong to the original ACP process. HTTP ids also cannot
  // be loaded by the CLI; keep old history without offering a lossy upgrade.
  if (input.agentId === 'kiro') return 'newChat';
  return input.nativeSessionId?.trim() ? 'continue' : 'newChat';
}

export function grokShouldFlushFollowUp(
  previousPhase: RuntimePhase | null | undefined,
  nextPhase: RuntimePhase,
): boolean {
  if (!previousPhase || !isRuntimeActive(previousPhase)) return false;
  return nextPhase === 'completed';
}
