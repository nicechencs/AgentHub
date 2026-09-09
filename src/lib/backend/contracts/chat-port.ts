import type { AgentKey, ChatEvent, ChatMessage, Conversation } from '@/lib/types';
import type { RuntimeOptions, RuntimeReply, RuntimeSnapshot, RuntimeStartExtras, RuntimeTurnSettings } from './chat-runtime';

export type MarkdownFilePreviewDto = {
  path: string;
  name: string;
  content: string;
  truncated: boolean;
};

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
  runtimeOptions(conversationId: string, opts?: { refresh?: boolean }): Promise<RuntimeOptions>;
  runtimeSetSettings(conversationId: string, settings: RuntimeTurnSettings): Promise<RuntimeTurnSettings>;
  runtimeNoteThinkingFailure(conversationId: string, settings: RuntimeTurnSettings, errorText: string): Promise<void>;
  runtimeContinueLegacy(conversationId: string): Promise<RuntimeSnapshot>;
  runtimeStart(conversationId: string, prompt: string, clientRequestId: string, extras?: RuntimeStartExtras): Promise<RuntimeSnapshot>;
  runtimeReply(reply: RuntimeReply): Promise<void>;
  runtimeSteer(conversationId: string, runId: string, prompt: string, clientRequestId: string): Promise<void>;
  runtimeCancel(conversationId: string, runId: string): Promise<void>;
  setChatModel(agentId: AgentKey, model: string): Promise<void>;
  setChatEffort(agentId: AgentKey, effort: string): Promise<void>;
  getChatModel(agentId: AgentKey): Promise<{
    model: string | null;
    models: string[];
    effort?: string | null;
    efforts?: string[];
  }>;
  pickChatImages(title?: string): Promise<string[]>;
  saveChatPasteImage(input: { base64: string; extension: string; byteLength?: number }): Promise<string>;
  /** Read a markdown file under the conversation working directory for the right-hand preview. */
  readMarkdownPreview(path: string, cwd: string): Promise<MarkdownFilePreviewDto>;
  /** Desktop menu accel (Ctrl/Cmd+N). Browser mock is a no-op. */
  onNativeShortcut(handler: (action: 'newChat') => void): Promise<() => void>;
}
