/**
 * Chat API façade — delegates to app runtime backend.
 */
import { getBackend } from '@/app/runtime';
import type { AgentKey, ChatEvent, ChatMessage, Conversation } from '@/lib/types';
import type { MarkdownFilePreviewDto } from '@/lib/backend/contracts/chat-port';
import type { RuntimeOptions, RuntimeReply, RuntimeSnapshot, RuntimeStartExtras, RuntimeTurnSettings } from '@/lib/backend/contracts/chat-runtime';
export type { MarkdownFilePreviewDto } from '@/lib/backend/contracts/chat-port';
export type { RuntimeQuestion, RuntimeRequest, RuntimeReply, RuntimeSnapshot, RuntimeOptions, RuntimeTurnSettings, RuntimeStartExtras, RuntimeModelOption, RuntimeExtensionItem } from '@/lib/backend/contracts/chat-runtime';

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
  agentIds: AgentKey[],
  cwd?: string | null,
): Promise<Conversation> {
  return getBackend().chat.createConversation(agentIds, cwd);
}

export async function ensureDefaultConversation(
  agentIds: AgentKey[],
  cwd?: string | null,
): Promise<Conversation> {
  return getBackend().chat.ensureDefaultConversation(agentIds, cwd);
}

export async function updateConversation(
  id: string,
  patch: {
    title?: string;
    agentIds?: AgentKey[];
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
export async function runtimeSnapshot(conversationId: string, afterSequence?: number): Promise<RuntimeSnapshot> {
  return getBackend().chat.runtimeSnapshot(conversationId, afterSequence);
}
export async function runtimeOptions(
  conversationId: string,
  opts?: { refresh?: boolean },
): Promise<RuntimeOptions> {
  return getBackend().chat.runtimeOptions(conversationId, opts);
}
export async function runtimeSetSettings(conversationId: string, settings: RuntimeTurnSettings): Promise<RuntimeTurnSettings> {
  return getBackend().chat.runtimeSetSettings(conversationId, settings);
}
export async function runtimeNoteThinkingFailure(conversationId: string, settings: RuntimeTurnSettings, errorText: string): Promise<void> {
  return getBackend().chat.runtimeNoteThinkingFailure(conversationId, settings, errorText);
}
export async function runtimeContinueLegacy(conversationId: string): Promise<RuntimeSnapshot> {
  return getBackend().chat.runtimeContinueLegacy(conversationId);
}
export async function runtimeStart(conversationId: string, prompt: string, clientRequestId: string, extras?: RuntimeStartExtras): Promise<RuntimeSnapshot> {
  return getBackend().chat.runtimeStart(conversationId, prompt, clientRequestId, extras);
}
export async function runtimeReply(reply: RuntimeReply): Promise<void> { return getBackend().chat.runtimeReply(reply); }
export async function runtimeSteer(conversationId: string, runId: string, prompt: string, clientRequestId: string): Promise<void> {
  return getBackend().chat.runtimeSteer(conversationId, runId, prompt, clientRequestId);
}
export async function runtimeCancel(conversationId: string, runId: string): Promise<void> {
  return getBackend().chat.runtimeCancel(conversationId, runId);
}

export async function setChatModel(agentId: AgentKey, model: string): Promise<void> {
  return getBackend().chat.setChatModel(agentId, model);
}

export async function setChatEffort(agentId: AgentKey, effort: string): Promise<void> {
  return getBackend().chat.setChatEffort(agentId, effort);
}

export async function getChatModel(
  agentId: AgentKey,
): Promise<{ model: string | null; models: string[]; effort?: string | null; efforts?: string[] }> {
  return getBackend().chat.getChatModel(agentId);
}


export async function pickChatImages(title?: string): Promise<string[]> {
  return getBackend().chat.pickChatImages(title);
}

export async function saveChatPasteImage(input: {
  base64: string;
  extension: string;
  byteLength?: number;
}): Promise<string> {
  return getBackend().chat.saveChatPasteImage(input);
}

export async function readMarkdownPreview(
  path: string,
  cwd: string,
): Promise<MarkdownFilePreviewDto> {
  return getBackend().chat.readMarkdownPreview(path, cwd);
}
