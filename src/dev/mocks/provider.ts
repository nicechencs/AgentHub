import { resolveAgentMeta } from '@/config/agents';
import { PRESETS } from '@/config/presets';
import type { ProviderPort } from '@/lib/backend/contracts';
import { delay, randomLatency } from '@/dev/mocks/delay';
import type { AgentId, Provider } from '@/lib/types';
import { moveMockProviderToTrash } from './trash';

function defaultPresetId(agentId: AgentId): string {
  return PRESETS[agentId]?.[0]?.id ?? 'default';
}

const mockState: Record<AgentId, Provider[]> = {
  claude: [],
  codex: [],
  kimi: [],
  grok: [],
  pi: [],
  workbuddy: [],
  cursor: [],
};

let lastSwitch: { agentId: AgentId; fromId: string; toId: string } | null = null;

export function createMockProviderPort(): ProviderPort {
  return {
    async listProviders(agentId) {
      await delay(randomLatency());
      if (agentId) {
        return (mockState[agentId] ?? []).map((p) => ({ ...p }));
      }
      return (Object.keys(mockState) as AgentId[]).flatMap((id) =>
        (mockState[id] ?? []).map((p) => ({ ...p })),
      );
    },

    async upsertProvider(p) {
      await delay(randomLatency());
      const list = mockState[p.agentId] ?? (mockState[p.agentId] = []);
      const idx = list.findIndex((x) => x.id === p.id);
      if (idx >= 0) {
        list[idx] = { ...p };
        return { ...list[idx] };
      }
      list.push({ ...p });
      return { ...p };
    },

    async deleteProvider(agentId, providerId) {
      await delay(randomLatency());
      const removed = (mockState[agentId] ?? []).find((provider) => provider.id === providerId);
      if (removed) moveMockProviderToTrash(removed);
      mockState[agentId] = (mockState[agentId] ?? []).filter((p) => p.id !== providerId);
    },

    async importProviderLive(agentId, name) {
      await delay(randomLatency());
      const current = (mockState[agentId] ?? []).find((p) => p.isCurrent);
      const p: Provider = {
        id: `p-${Date.now()}`,
        agentId,
        name: name ?? '导入的配置',
        preset: current?.preset ?? defaultPresetId(agentId),
        configText: current?.configText ?? PRESETS[agentId][0]?.template ?? '{}',
        configFormat: current?.configFormat ?? PRESETS[agentId][0]?.format ?? 'json',
        isCurrent: true,
      };
      const list = mockState[agentId] ?? (mockState[agentId] = []);
      list.forEach((x) => {
        x.isCurrent = false;
      });
      list.push(p);
      return { ...p };
    },

    async switchPreview(agentId, _toProviderId) {
      await delay(150);
      const list = mockState[agentId] ?? [];
      const current = list.find((p) => p.isCurrent);
      const meta = resolveAgentMeta(agentId);
      return {
        backfillSummary: current
          ? `当前生效配置将回存为「${current.name}」`
          : '尚无生效配置，将直接写入本机',
        backupPath: `~/.agenthub/backups/live/${agentId}/`,
        processWarning:
          agentId === 'claude' ? `检测到 ${meta.name} 进程正在运行,切换后需重启生效` : undefined,
      };
    },

    async switchProvider(agentId, toProviderId) {
      await delay(randomLatency());
      const list = mockState[agentId] ?? [];
      const from = list.find((p) => p.isCurrent);
      lastSwitch = { agentId, fromId: from?.id ?? '', toId: toProviderId };
      list.forEach((p) => {
        p.isCurrent = p.id === toProviderId;
      });
    },

    async undoSwitch(agentId) {
      await delay(200);
      if (!lastSwitch || lastSwitch.agentId !== agentId) return false;
      const list = mockState[agentId] ?? [];
      list.forEach((p) => {
        p.isCurrent = p.id === lastSwitch!.fromId;
      });
      lastSwitch = null;
      return true;
    },

    async testLatency(_agentId, _providerId) {
      await delay(600 + Math.random() * 800);
      return Math.round(60 + Math.random() * 320);
    },

    async listProviderPresets(agentId) {
      if (agentId) {
        return PRESETS[agentId].map((p) => ({ agent: agentId, ...p }));
      }
      return (Object.keys(PRESETS) as AgentId[]).flatMap((id) =>
        PRESETS[id].map((p) => ({ agent: id, ...p })),
      );
    },
  };
}

export function restoreMockProvider(provider: Provider): void {
  const list = mockState[provider.agentId];
  if (list.some((item) => item.id === provider.id)) {
    throw new Error(`provider already exists: ${provider.id}`);
  }
  list.push({ ...provider, isCurrent: false });
}
