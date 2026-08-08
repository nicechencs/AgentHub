import type { ChatPort } from '@/lib/backend/contracts';
import {
  mapChatMessage,
  mapConversation,
  type CoreChatEvent,
  type CoreChatMessage,
  type CoreConversation,
} from '@/lib/backend/contracts/chat-map';
import { Channel, invoke } from './invoke';

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
  };
}
