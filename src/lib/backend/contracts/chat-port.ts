import type { AgentKey, ChatEvent, ChatMessage, Conversation } from '@/lib/types';
import type { RuntimeReply, RuntimeSnapshot } from './chat-runtime';

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
  runtimeSnapshot(conversationId: string, afterSequence?: number): Promise<RuntimeSnapshot>;
  runtimeStart(conversationId: string, prompt: string, clientRequestId: string): Promise<RuntimeSnapshot>;
  runtimeReply(reply: RuntimeReply): Promise<void>;
  runtimeSteer(conversationId: string, runId: string, prompt: string, clientRequestId: string): Promise<void>;
  runtimeCancel(conversationId: string, runId: string): Promise<void>;
  setChatModel(agentId: AgentKey, model: string): Promise<void>;
  getChatModel(agentId: AgentKey): Promise<{ model: string | null; models: string[] }>;
}
