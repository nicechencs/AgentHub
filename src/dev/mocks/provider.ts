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

/** Clears browser-mock Connection state so each backend factory starts clean. */
export function resetMockProviders(): void {
  (Object.keys(mockState) as AgentId[]).forEach((agentId) => {
    mockState[agentId].length = 0;
  });
  lastSwitch = null;
}

/** Read-only lookup used by browser-only compatibility previews. */
export function getMockProviderById(providerId: string): Provider | undefined {
  const found = (Object.keys(mockState) as AgentId[])
    .flatMap((agentId) => mockState[agentId] ?? [])
    .find((provider) => provider.id === providerId);
  return found ? { ...found } : undefined;
}

/** Snapshot of all mock providers (ticket wallet aggregation). */
export function listMockProviders(): Provider[] {
  return (Object.keys(mockState) as AgentId[]).flatMap((agentId) =>
    (mockState[agentId] ?? []).map((provider) => ({ ...provider })),
  );
}

function mockSecretTail(provider: Provider): string | undefined {
  if (provider.secretTail?.trim()) return provider.secretTail.trim();
  const fromAuth = provider.authApiKey?.trim();
  if (fromAuth && fromAuth !== '***' && fromAuth.length >= 8) {
    return `**${fromAuth.slice(-4)}`;
  }
  const match = (provider.configText ?? '').match(
    /(?:api[_-]?key|ANTHROPIC_AUTH_TOKEN|OPENAI_API_KEY)\s*[:=]\s*["']?([^\s"',]+)["']?/i,
  );
  const key = match?.[1]?.trim();
  if (key && key !== '***' && key.length >= 8) return `**${key.slice(-4)}`;
  return undefined;
}

/** Synchronous test-only insertion used when the mock Adapter generates a Connection. */
export function upsertMockProvider(provider: Provider): Provider {
  const list = mockState[provider.agentId] ?? (mockState[provider.agentId] = []);
  const index = list.findIndex((item) => item.id === provider.id);
  const next = { ...provider, secretTail: mockSecretTail(provider) };
  if (provider.isCurrent) {
    list.forEach((item) => {
      item.isCurrent = false;
    });
  }
  if (index >= 0) {
    list[index] = next;
  } else {
    list.push(next);
  }
  return { ...(index >= 0 ? list[index] : list[list.length - 1]) };
}

/** Synchronous cleanup used only by Adapter-owned generated Connections. */
export function removeMockProvider(provider: Provider): void {
  const list = mockState[provider.agentId] ?? [];
  mockState[provider.agentId] = list.filter((item) => item.id !== provider.id);
  if (lastSwitch?.agentId === provider.agentId
    && (lastSwitch.fromId === provider.id || lastSwitch.toId === provider.id)) {
    lastSwitch = null;
  }
}

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
      return upsertMockProvider(p);
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
