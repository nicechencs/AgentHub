import type { AgentId, ChatEvent, ChatMessage, Conversation } from '@/lib/types';

export interface ChatPort {
  listConversations(): Promise<Conversation[]>;
  createConversation(agentIds: AgentId[], cwd?: string | null): Promise<Conversation>;
  ensureDefaultConversation(agentIds: AgentId[], cwd?: string | null): Promise<Conversation>;
  updateConversation(
    id: string,
    patch: {
      title?: string;
      agentIds?: AgentId[];
      cwd?: string | null;
      allowDangerous?: boolean;
    },
  ): Promise<Conversation>;
  deleteConversation(id: string): Promise<void>;
  listChatMessages(conversationId: string): Promise<ChatMessage[]>;
  chatSend(
    conversationId: string,
    prompt: string,
    onEvent: (ev: ChatEvent) => void,
  ): Promise<void>;
  chatCancel(conversationId: string): Promise<void>;
  setChatModel(agentId: AgentId, model: string): Promise<void>;
}
