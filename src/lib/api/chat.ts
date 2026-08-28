/**
 * Chat API façade — delegates to app runtime backend.
 */
import { getBackend } from '@/app/runtime';
import type { AgentId, ChatEvent, ChatMessage, Conversation } from '@/lib/types';

export type {
  CoreConversation,
  CoreChatMessage,
  CoreChatEvent,
} from '@/lib/backend/contracts/chat-map';
export { mapConversation, mapChatMessage } from '@/lib/backend/contracts/chat-map';

export async function listConversations(): Promise<Conversation[]> {
  return getBackend().chat.listConversations();
}

export async function createConversation(
  agentIds: AgentId[],
  cwd?: string | null,
): Promise<Conversation> {
  return getBackend().chat.createConversation(agentIds, cwd);
}

export async function ensureDefaultConversation(
  agentIds: AgentId[],
  cwd?: string | null,
): Promise<Conversation> {
  return getBackend().chat.ensureDefaultConversation(agentIds, cwd);
}

export async function updateConversation(
  id: string,
  patch: {
    title?: string;
    agentIds?: AgentId[];
    cwd?: string | null;
    allowDangerous?: boolean;
  },
): Promise<Conversation> {
  return getBackend().chat.updateConversation(id, patch);
}

export async function deleteConversation(id: string): Promise<void> {
  return getBackend().chat.deleteConversation(id);
}

export async function listChatMessages(conversationId: string): Promise<ChatMessage[]> {
  return getBackend().chat.listChatMessages(conversationId);
}

export async function chatSend(
  conversationId: string,
  prompt: string,
  onEvent: (ev: ChatEvent) => void,
): Promise<void> {
  return getBackend().chat.chatSend(conversationId, prompt, onEvent);
}

export async function chatCancel(conversationId: string): Promise<void> {
  return getBackend().chat.chatCancel(conversationId);
}

export async function setChatModel(agentId: AgentId, model: string): Promise<void> {
  return getBackend().chat.setChatModel(agentId, model);
}
