import type { AgentKey, ChatEvent, ChatMessage, Conversation } from '@/lib/types';

export interface ChatPort {
  listConversations(): Promise<Conversation[]>;
  createConversation(agentIds: AgentKey[], cwd?: string | null): Promise<Conversation>;
  ensureDefaultConversation(agentIds: AgentKey[], cwd?: string | null): Promise<Conversation>;
  updateConversation(
    id: string,
    patch: {
      title?: string;
      agentIds?: AgentKey[];
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
  setChatModel(agentId: AgentKey, model: string): Promise<void>;
  getChatModel(agentId: AgentKey): Promise<{ model: string | null; models: string[] }>;
}
