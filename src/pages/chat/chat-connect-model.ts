import type { RuntimeChannel } from '@/lib/backend/contracts/chat-runtime';
import type { MessageKey } from '@/lib/i18n';

/** User-facing connection for this chat / this Agent's new chats. */
export type ChatConnectKind = 'acp' | 'continuous' | 'legacy';

/** What a new empty chat for this Agent uses. Cursor stays off this list. */
export function agentNewChatConnectKind(
  agentId: string | null | undefined,
): ChatConnectKind {
  if (agentId === 'grok' || agentId === 'kiro') return 'acp';
  if (agentId === 'codex' || agentId === 'claude') return 'continuous';
  return 'legacy';
}

/**
 * What *this* conversation is using.
 * Prefer the live Options channel; fall back to the Agent's new-chat default
 * only while a continuous chat is already enabled.
 */
export function sessionChatConnectKind(input: {
  agentId?: string | null;
  transport?: RuntimeChannel | null;
  runtimeEnabled?: boolean | null;
}): ChatConnectKind {
  if (input.transport === 'acp') return 'acp';
  if (input.transport === 'app-server' || input.transport === 'stream-json') {
    return 'continuous';
  }
  if (input.runtimeEnabled) return agentNewChatConnectKind(input.agentId);
  return 'legacy';
}

export function chatConnectLabelKey(kind: ChatConnectKind): MessageKey {
  if (kind === 'acp') return 'chat.connect.acp';
  if (kind === 'continuous') return 'chat.connect.continuous';
  return 'chat.connect.legacy';
}

export function agentChatConnectHintKey(kind: ChatConnectKind): MessageKey {
  if (kind === 'acp') return 'chat.connect.agentHintAcp';
  if (kind === 'continuous') return 'chat.connect.agentHintContinuous';
  return 'chat.connect.agentHintLegacy';
}
