import type { ChatPort } from '@/lib/backend/contracts';
import { delay } from '@/dev/mocks/delay';
import type {
  AgentId,
  ChatEvent,
  ChatMessage,
  ChatMessageStatus,
  Conversation,
} from '@/lib/types';

let mockSeq = 1;
const mockConversations: Conversation[] = [];
const mockMessages: Record<string, ChatMessage[]> = {};
const mockCancel = new Set<string>();

function nowIso() {
  return new Date().toISOString();
}

function requireSingleAgent(agentIds: AgentId[]): AgentId[] {
  const seen: AgentId[] = [];
  for (const id of agentIds) {
    if (!seen.includes(id)) seen.push(id);
  }
  if (seen.length === 0) {
    throw new Error('conversation must select at least one agent');
  }
  if (seen.length > 1) {
    throw new Error('conversation can select only one agent');
  }
  return seen;
}

function mockTitle(prompt: string) {
  const t = prompt.trim();
  return t.length <= 30 ? t : `${t.slice(0, 29)}…`;
}

export function resetChatMock() {
  mockSeq = 1;
  mockConversations.length = 0;
  for (const k of Object.keys(mockMessages)) delete mockMessages[k];
  mockCancel.clear();
}

export function createMockChatPort(): ChatPort {
  return {
    async listConversations() {
      await delay(120);
      return mockConversations.map((c) => ({ ...c }));
    },

    async createConversation(agentIds, cwd) {
      await delay(120);
      const conv: Conversation = {
        id: `conv-mock-${mockSeq++}`,
        title: '',
        agentIds: requireSingleAgent(agentIds),
        cwd: cwd ?? null,
        allowDangerous: false,
        createdAt: nowIso(),
        updatedAt: nowIso(),
        nativeSessionId: null,
      };
      mockConversations.unshift(conv);
      mockMessages[conv.id] = [];
      return { ...conv };
    },

    async updateConversation(id, patch) {
      await delay(80);
      const idx = mockConversations.findIndex((c) => c.id === id);
      if (idx < 0) throw new Error(`conversation not found: ${id}`);
      const cur = mockConversations[idx];
      const agentIds = patch.agentIds ? requireSingleAgent(patch.agentIds) : cur.agentIds;
      const cwd = patch.cwd !== undefined ? patch.cwd : cur.cwd;
      const resetNative =
        JSON.stringify(agentIds) !== JSON.stringify(cur.agentIds) || cwd !== cur.cwd;
      const next: Conversation = {
        ...cur,
        title: patch.title ?? cur.title,
        agentIds,
        cwd,
        allowDangerous: patch.allowDangerous ?? cur.allowDangerous,
        nativeSessionId: resetNative ? null : cur.nativeSessionId,
        updatedAt: nowIso(),
      };
      mockConversations[idx] = next;
      return { ...next };
    },

    async deleteConversation(id) {
      await delay(80);
      const i = mockConversations.findIndex((c) => c.id === id);
      if (i >= 0) mockConversations.splice(i, 1);
      delete mockMessages[id];
    },

    async listChatMessages(conversationId) {
      await delay(80);
      return (mockMessages[conversationId] ?? []).map((m) => ({ ...m }));
    },

    async chatSend(conversationId, prompt, onEvent: (ev: ChatEvent) => void) {
      const conv = mockConversations.find((c) => c.id === conversationId);
      if (!conv) throw new Error(`conversation not found: ${conversationId}`);
      const msgs = mockMessages[conversationId] ?? (mockMessages[conversationId] = []);
      const turn = msgs.reduce((max, m) => Math.max(max, m.turn), 0) + 1;
      const userMsg: ChatMessage = {
        id: `msg-mock-${mockSeq++}`,
        conversationId,
        turn,
        role: 'user',
        content: prompt,
        status: 'ok',
        durationMs: 0,
        createdAt: nowIso(),
      };
      msgs.push(userMsg);
      if (!conv.title) {
        conv.title = mockTitle(prompt);
      }
      conv.updatedAt = nowIso();

      const agents = conv.agentIds.slice(0, 1);
      onEvent({ type: 'started', turn, agents });
      mockCancel.delete(conversationId);

      for (const agent of agents) {
        if (mockCancel.has(conversationId)) {
          const cancelled: ChatMessage = {
            id: `msg-mock-${mockSeq++}`,
            conversationId,
            turn,
            role: 'agent',
            agentId: agent,
            content: '',
            status: 'cancelled',
            durationMs: 0,
            error: 'cancelled',
            createdAt: nowIso(),
          };
          msgs.push(cancelled);
          onEvent({ type: 'agentFinished', turn, agent, message: cancelled });
          continue;
        }

        onEvent({ type: 'agentStarted', turn, agent, command: `${agent} -p …` });
        onEvent({
          type: 'agentChunk',
          turn,
          agent,
          stream: 'stderr',
          text: `[mock] starting ${agent} headless run\n`,
        });
        onEvent({
          type: 'agentProcess',
          turn,
          agent,
          step: { type: 'status', phase: 'starting', detail: 'mock-session' },
        });
        onEvent({
          type: 'agentProcess',
          turn,
          agent,
          step: {
            type: 'tool',
            id: `mock-tool-${agent}`,
            name: 'Read',
            status: 'start',
            input: { path: 'README.md' },
          },
        });
        await delay(80);
        onEvent({
          type: 'agentProcess',
          turn,
          agent,
          step: {
            type: 'tool',
            id: `mock-tool-${agent}`,
            name: 'Read',
            status: 'end',
            result: '…(mock file excerpt)…',
          },
        });
        onEvent({
          type: 'agentProcess',
          turn,
          agent,
          step: { type: 'thinking', text: '规划回复结构…', done: false },
        });
        const parts = [
          `【${agent} mock】收到：${prompt.slice(0, 80)}\n`,
          '正在思考…\n',
          `这是 ${agent} 的模拟回复（浏览器 Vite 原型，未调用真实 CLI）。\n`,
        ];
        let content = '';
        for (const part of parts) {
          if (mockCancel.has(conversationId)) break;
          await delay(180 + Math.random() * 120);
          content += part;
          onEvent({ type: 'agentChunk', turn, agent, stream: 'stdout', text: part });
        }
        const status: ChatMessageStatus = mockCancel.has(conversationId) ? 'cancelled' : 'ok';
        const finished: ChatMessage = {
          id: `msg-mock-${mockSeq++}`,
          conversationId,
          turn,
          role: 'agent',
          agentId: agent,
          content,
          status,
          durationMs: 500,
          error: status === 'cancelled' ? 'cancelled' : null,
          createdAt: nowIso(),
        };
        msgs.push(finished);
        onEvent({ type: 'agentFinished', turn, agent, message: finished });
        if (!conv.nativeSessionId && (agent === 'claude' || agent === 'codex')) {
          conv.nativeSessionId = `mock-session-${conv.id}`;
        }
      }

      onEvent({ type: 'finished', turn, ok: !mockCancel.has(conversationId) });
      mockCancel.delete(conversationId);
    },

    async chatCancel(conversationId) {
      mockCancel.add(conversationId);
    },
  };
}
