import type { ChatPort } from '@/lib/backend/contracts';
import type { RuntimeOptions, RuntimeReply, RuntimeSnapshot, RuntimeStartExtras, RuntimeTurnSettings } from '@/lib/backend/contracts/chat-runtime';
import { delay } from '@/dev/mocks/delay';
import type {
  AgentKey,
  ChatEvent,
  ChatMessage,
  ChatMessageStatus,
  Conversation,
} from '@/lib/types';

let mockSeq = 1;
const mockConversations: Conversation[] = [];
const mockMessages: Record<string, ChatMessage[]> = {};
const mockCancel = new Set<string>();
const mockInflight = new Set<string>();
const runtimeSnapshots = new Map<string, RuntimeSnapshot>();
const runtimeSettings = new Map<string, RuntimeTurnSettings>();
const runtimeDeniedEfforts = new Map<string, Set<string>>();
const runtimeOptionsCache = new Map<string, RuntimeOptions>();

function nowIso() {
  return new Date().toISOString();
}

function requireSingleAgent(agentIds: AgentKey[]): AgentKey[] {
  const seen: AgentKey[] = [];
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


function applyMockDeniedEfforts<T extends { id: string; efforts: string[]; defaultEffort?: string | null }>(
  models: T[],
): T[] {
  return models.map((option) => {
    const blocked = runtimeDeniedEfforts.get(option.id);
    if (!blocked || blocked.size === 0) return option;
    const efforts = option.efforts.filter((item) => !blocked.has(item));
    const fallback = option.defaultEffort?.trim();
    const defaultEffort =
      fallback && efforts.includes(fallback) ? fallback : efforts[0] ?? null;
    return { ...option, efforts, defaultEffort };
  });
}

function learnMockThinkingUnsupported(
  conversationId: string,
  errorText: string,
  explicitSettings?: RuntimeTurnSettings,
) {
  const hay = errorText.toLowerCase();
  if (
    !(
      hay.includes('reasoningeffort')
      || hay.includes('reasoning_effort')
      || hay.includes('does not support parameter')
      || hay.includes('不支持思考强度')
      || hay.includes('不支持当前思考设置')
    )
  ) {
    return;
  }
  const settings = explicitSettings ?? runtimeSettings.get(conversationId) ?? {};
  const model = settings.model?.trim();
  const effort = settings.effort?.trim();
  if (!model || !effort) return;
  const set = runtimeDeniedEfforts.get(model) ?? new Set<string>();
  set.add(effort);
  runtimeDeniedEfforts.set(model, set);
  const cached = runtimeOptionsCache.get(conversationId);
  if (cached) {
    runtimeOptionsCache.set(conversationId, {
      ...cached,
      models: applyMockDeniedEfforts(cached.models),
    });
  }
}

export function resetChatMock() {
  mockSeq = 1;
  mockConversations.length = 0;
  for (const k of Object.keys(mockMessages)) delete mockMessages[k];
  mockCancel.clear();
  mockInflight.clear();
  runtimeSnapshots.clear();
  runtimeSettings.clear();
  runtimeOptionsCache.clear();
  runtimeDeniedEfforts.clear();
}

export function createMockChatPort(): ChatPort {
  return {
    async listConversations() {
      await delay(120);
      return mockConversations.map((c) => ({ ...c, sending: mockInflight.has(c.id) }));
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

    async ensureDefaultConversation(agentIds, cwd) {
      await delay(120);
      const normalizedAgentIds = requireSingleAgent(agentIds);
      const existing = mockConversations.find(
        (c) => c.title.trim() === '' && (mockMessages[c.id] ?? []).length === 0,
      );
      if (existing) {
        return { ...existing, sending: mockInflight.has(existing.id) };
      }
      const conv: Conversation = {
        id: `conv-mock-${mockSeq++}`,
        title: '',
        agentIds: normalizedAgentIds,
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
      mockInflight.delete(id);
      mockCancel.delete(id);
    },

    async listChatMessages(conversationId) {
      await delay(80);
      return (mockMessages[conversationId] ?? []).map((m) => ({ ...m }));
    },

    async chatSend(conversationId, prompt, onEvent: (ev: ChatEvent) => void) {
      const conv = mockConversations.find((c) => c.id === conversationId);
      if (!conv) throw new Error(`conversation not found: ${conversationId}`);
      mockInflight.add(conversationId);
      try {
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

        const cancelled = mockCancel.has(conversationId);
        onEvent({ type: 'finished', turn, ok: true, cancelled });
      } finally {
        mockInflight.delete(conversationId);
        mockCancel.delete(conversationId);
      }
    },

    async chatCancel(conversationId) {
      mockCancel.add(conversationId);
    },

    async runtimeSnapshot(conversationId, afterSequence) {
      const conv = mockConversations.find((item) => item.id === conversationId);
      if (!conv) throw new Error(`conversation not found: ${conversationId}`);
      const current = runtimeSnapshots.get(conversationId) ?? {
        conversationId,
        enabled: conv.agentIds[0] === 'codex' && (mockMessages[conversationId] ?? []).length === 0,
        runId: null,
        phase: 'idle' as const,
        lastSequence: 0,
        events: [],
        pendingRequests: [],
        gap: false,
        currentMessage: null,
      };
      return { ...current, events: afterSequence == null ? current.events : current.events.filter((item) => item.sequence > afterSequence) };
    },

    async runtimeOptions(conversationId) {
      const snapshot = await this.runtimeSnapshot(conversationId);
      if (!snapshot.enabled) throw new Error('runtime is unavailable for this conversation');
      const frozen = ['starting', 'running', 'waiting', 'cancelling'].includes(snapshot.phase);
      const cached = runtimeOptionsCache.get(conversationId);
      if (cached) {
        const models = applyMockDeniedEfforts(cached.models);
        let settings = runtimeSettings.get(conversationId) ?? cached.settings;
        if (!frozen && models.length > 0 && settings.model) {
          const modelOption = models.find((item) => item.id === settings.model);
          const effort = settings.effort?.trim() || undefined;
          if (
            modelOption &&
            effort &&
            (modelOption.efforts.length === 0 || !modelOption.efforts.includes(effort))
          ) {
            const defaultEffort =
              modelOption.defaultEffort && modelOption.efforts.includes(modelOption.defaultEffort)
                ? modelOption.defaultEffort
                : modelOption.efforts[0] ?? null;
            settings = { model: settings.model, effort: defaultEffort };
            runtimeSettings.set(conversationId, settings);
          }
        }
        return {
          ...cached,
          models,
          settings,
          settingsFrozen: frozen,
        };
      }
      // Match core: never invent a catalog mid-turn when nothing was prefetched.
      if (frozen) {
        return {
          conversationId,
          settings: runtimeSettings.get(conversationId) ?? {},
          settingsFrozen: true,
          models: [],
          extensions: [],
          modelsFromCodex: false,
        };
      }
      const options: RuntimeOptions = {
        conversationId,
        settings: runtimeSettings.get(conversationId) ?? {},
        settingsFrozen: false,
        models: [
          { id: 'gpt-mock', efforts: ['low', 'medium', 'high'], defaultEffort: 'medium' },
          // Live Codex 0.150+ over-reports medium/xhigh for spark; learn-from-reject filters later.
          { id: 'gpt-5.3-codex-spark', efforts: ['low', 'medium', 'high', 'xhigh'], defaultEffort: 'high' },
        ],
        extensions: [
          {
            id: '/mock/skills/demo/SKILL.md',
            name: 'demo',
            kind: 'skill',
            installed: true,
            enabled: true,
            loaded: false,
            callable: true,
            path: '/mock/skills/demo/SKILL.md',
          },
        ],
        modelsFromCodex: false,
      };
      options.models = applyMockDeniedEfforts(options.models);
      runtimeOptionsCache.set(conversationId, options);
      return options;
    },
    async runtimeSetSettings(conversationId, settings) {
      const snapshot = await this.runtimeSnapshot(conversationId);
      if (!snapshot.enabled) throw new Error('runtime is unavailable for this conversation');
      if (['starting', 'running', 'waiting', 'cancelling'].includes(snapshot.phase)) {
        throw new Error('当前轮次进行中，不能修改模型或思考强度');
      }
      const options = await this.runtimeOptions(conversationId);
      const model = settings.model?.trim() || undefined;
      const effort = settings.effort?.trim() || undefined;
      if (!model && effort) {
        throw new Error('选择思考强度前需要先选择模型');
      }
      if (model && !options.models.some((item) => item.id === model)) {
        throw new Error(`模型不可用: ${model}`);
      }
      const modelOption = options.models.find((item) => item.id === model);
      const defaultEffort =
        modelOption && modelOption.defaultEffort && modelOption.efforts.includes(modelOption.defaultEffort)
          ? modelOption.defaultEffort
          : modelOption?.efforts[0] ?? null;
      if (model && effort) {
        if (!modelOption || modelOption.efforts.length === 0) {
          throw new Error(`模型 ${model} 不支持思考强度`);
        }
        if (!modelOption.efforts.includes(effort)) {
          throw new Error(`模型 ${model} 不支持思考强度 ${effort}`);
        }
      }
      const next: RuntimeTurnSettings = {
        model: model ?? null,
        effort: model ? (effort ?? defaultEffort) : null,
      };
      runtimeSettings.set(conversationId, next);
      return next;
    },
    async runtimeNoteThinkingFailure(conversationId, settings, errorText) {
      learnMockThinkingUnsupported(conversationId, errorText, settings);
    },
    async runtimeStart(conversationId, prompt, _clientRequestId, extras?: RuntimeStartExtras) {
      const snapshot = await this.runtimeSnapshot(conversationId);
      if (!snapshot.enabled) throw new Error('runtime is unavailable for this conversation');
      for (const image of extras?.images ?? []) {
        if (!image.path.trim()) throw new Error('图片路径不能为空');
      }
      // Validate against an already-warmed catalog only — do not fetch/cache here
      // or cold mid-turn options() would stop matching core's empty-catalog behavior.
      const warmed = runtimeOptionsCache.get(conversationId);
      const settings = runtimeSettings.get(conversationId) ?? warmed?.settings ?? {};
      const model = settings.model?.trim() || undefined;
      const effort = settings.effort?.trim() || undefined;
      const catalog = warmed?.models ?? [];
      if (catalog.length > 0 && model) {
        const modelOption = catalog.find((item) => item.id === model);
        if (!modelOption) throw new Error(`模型不可用: ${model}`);
        if (effort) {
          if (modelOption.efforts.length === 0) {
            throw new Error(`模型 ${model} 不支持思考强度`);
          }
          if (!modelOption.efforts.includes(effort)) {
            throw new Error(`模型 ${model} 不支持思考强度 ${effort}`);
          }
        }
      }
      // Simulate Codex rejecting over-reported spark+medium (live accept failure).
      if (model === 'gpt-5.3-codex-spark' && effort === 'medium') {
        const message =
          'OpenAI API error (400): does not support parameter reasoningEffort=medium';
        learnMockThinkingUnsupported(conversationId, message);
        throw new Error(message);
      }
      const runId = `run-mock-${mockSeq++}`;
      const agent = mockConversations.find((item) => item.id === conversationId)?.agentIds[0] ?? 'codex';
      const turn = (mockMessages[conversationId] ?? []).length + 1;
      const event: ChatEvent = { type: 'started', turn, agents: [agent] };
      const currentMessage: ChatMessage = {
        id: `runtime-agent-${mockSeq++}`,
        conversationId,
        turn,
        role: 'agent',
        agentId: agent,
        content: '',
        status: 'running',
        durationMs: 0,
        createdAt: nowIso(),
      };
      const next: RuntimeSnapshot = {
        ...snapshot,
        runId,
        phase: 'running',
        lastSequence: 1,
        events: [{ sequence: 1, event }],
        pendingRequests: [],
        currentMessage,
      };
      runtimeSnapshots.set(conversationId, next);
      void prompt;
      return next;
    },
    async runtimeReply(_reply: RuntimeReply) {},
    async runtimeSteer() {},
    async runtimeCancel(conversationId, runId) {
      const current = runtimeSnapshots.get(conversationId);
      if (current?.runId !== runId) throw new Error('run is no longer active');
      runtimeSnapshots.set(conversationId, {
        ...current,
        phase: 'cancelling',
        currentMessage: current.currentMessage
          ? { ...current.currentMessage, status: 'cancelled', error: 'cancelled' }
          : null,
      });
    },

    async setChatModel(_agentId, _model) {
      await delay(40);
    },
    async setChatEffort(_agentId, _effort) {
      await delay(40);
    },
    async getChatModel(agentId) {
      await delay(20);
      if (agentId === 'grok') {
        return {
          model: 'grok-4.6',
          models: ['grok-4.6', 'grok-4.5'],
          effort: 'high',
          efforts: ['low', 'high', 'xhigh'],
        };
      }
      return { model: null, models: [], effort: null, efforts: [] };
    },
    async pickChatImages() {
      await delay(10);
      return ['/tmp/mock-chat.png'];
    },
    async saveChatPasteImage(input) {
      await delay(5);
      const ext = input.extension.replace(/^\./, '') || 'png';
      return `/tmp/mock-paste.${ext}`;
    },
  };
}
