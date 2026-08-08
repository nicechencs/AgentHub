import type { ChatEvent, ChatMessage, Conversation } from '@/lib/types';

export type CoreConversation = Conversation;
export type CoreChatMessage = ChatMessage;
export type CoreChatEvent = ChatEvent;

export function mapConversation(c: CoreConversation): Conversation {
  return { ...c };
}

export function mapChatMessage(m: CoreChatMessage): ChatMessage {
  return { ...m };
}
