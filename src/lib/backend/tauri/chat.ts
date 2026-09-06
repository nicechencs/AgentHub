import type { ChatPort } from '@/lib/backend/contracts';
import {
  mapChatMessage,
  mapConversation,
  type CoreChatEvent,
  type CoreChatMessage,
  type CoreConversation,
} from '@/lib/backend/contracts/chat-map';
import { Channel, invoke } from './invoke';
import type { RuntimeOptions, RuntimeSnapshot, RuntimeTurnSettings } from '@/lib/backend/contracts/chat-runtime';

export function createTauriChatPort(): ChatPort {
  return {
    async listConversations() {
      const rows = await invoke<CoreConversation[]>('list_conversations');
      return rows.map(mapConversation);
    },

    async createConversation(agentIds, cwd) {
      const row = await invoke<CoreConversation>('create_conversation', {
        agentIds,
        cwd: cwd ?? null,
      });
      return mapConversation(row);
    },

    async ensureDefaultConversation(agentIds, cwd) {
      const row = await invoke<CoreConversation>('ensure_default_conversation', {
        agentIds,
        cwd: cwd ?? null,
      });
      return mapConversation(row);
    },

    async updateConversation(id, patch) {
      const cwdArg =
        patch.cwd === undefined ? null : patch.cwd === null || patch.cwd === '' ? '' : patch.cwd;
      const row = await invoke<CoreConversation>('update_conversation', {
        id,
        title: patch.title ?? null,
        agentIds: patch.agentIds ?? null,
        cwd: cwdArg,
        allowDangerous: patch.allowDangerous ?? null,
      });
      return mapConversation(row);
    },

    async deleteConversation(id) {
      await invoke('delete_conversation', { id });
    },

    async listChatMessages(conversationId) {
      const rows = await invoke<CoreChatMessage[]>('list_chat_messages', { conversationId });
      return rows.map(mapChatMessage);
    },

    async chatSend(conversationId, prompt, onEvent) {
      const ch = new Channel<CoreChatEvent>();
      ch.onmessage = (ev) => onEvent(ev);
      await invoke('chat_send', { conversationId, prompt, onEvent: ch });
    },

    async chatCancel(conversationId) {
      await invoke('chat_cancel', { conversationId });
    },
    async runtimeSnapshot(conversationId, afterSequence) {
      return invoke<RuntimeSnapshot>('chat_runtime_snapshot', { conversationId, afterSequence });
    },
    async runtimeOptions(conversationId) {
      return invoke<RuntimeOptions>('chat_runtime_options', { conversationId });
    },
    async runtimeSetSettings(conversationId, settings) {
      return invoke<RuntimeTurnSettings>('chat_runtime_set_settings', { conversationId, settings });
    },
    async runtimeNoteThinkingFailure(conversationId, settings, errorText) {
      await invoke('chat_runtime_note_thinking_failure', { conversationId, settings, errorText });
    },
    async runtimeStart(conversationId, prompt, clientRequestId, extras) {
      return invoke<RuntimeSnapshot>('chat_runtime_start', {
        conversationId,
        prompt,
        clientRequestId,
        extras: extras ?? null,
      });
    },
    async runtimeReply(reply) { await invoke('chat_runtime_reply', { reply }); },
    async runtimeSteer(conversationId, runId, prompt, clientRequestId) {
      await invoke('chat_runtime_steer', { conversationId, runId, prompt, clientRequestId });
    },
    async runtimeCancel(conversationId, runId) {
      await invoke('chat_runtime_cancel', { conversationId, runId });
    },

    async setChatModel(agentId, model) {
      await invoke('set_chat_model', { agentId, model });
    },
    async setChatEffort(agentId, effort) {
      await invoke('set_chat_effort', { agentId, effort });
    },
    async getChatModel(agentId) {
      const row = await invoke<{
        model?: string | null;
        models?: string[];
        effort?: string | null;
        efforts?: string[];
      }>('get_chat_model', {
        agentId,
      });
      return {
        model: typeof row.model === 'string' && row.model.trim() ? row.model.trim() : null,
        models: Array.isArray(row.models)
          ? row.models.filter((id): id is string => typeof id === 'string' && Boolean(id.trim()))
          : [],
        effort: typeof row.effort === 'string' && row.effort.trim() ? row.effort.trim() : null,
        efforts: Array.isArray(row.efforts)
          ? row.efforts.filter((id): id is string => typeof id === 'string' && Boolean(id.trim()))
          : [],
      };
    },
    async pickChatImages(title) {
      return invoke<string[]>('pick_chat_images', { title: title ?? null });
    },
    async saveChatPasteImage(input) {
      return invoke<string>('save_chat_paste_image', {
        base64: input.base64,
        extension: input.extension,
        byteLength: input.byteLength ?? null,
      });
    },
  };
}
