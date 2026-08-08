import type { AccountPort } from '@/lib/backend/contracts';
import { delay, randomLatency } from '@/dev/mocks/delay';
import type { Account, AgentId } from '@/lib/types';

const mockState: Record<AgentId, Account[]> = {
  claude: [],
  codex: [],
  kimi: [],
  grok: [],
  pi: [],
  workbuddy: [],
  cursor: [],
};

let lastSwitch: { agentId: AgentId; fromId: string } | null = null;

export function createMockAccountPort(): AccountPort {
  return {
    async listAccounts(agentId) {
      await delay(randomLatency());
      if (agentId) {
        return (mockState[agentId] ?? []).map((a) => ({ ...a }));
      }
      return (Object.keys(mockState) as AgentId[]).flatMap((id) =>
        (mockState[id] ?? []).map((a) => ({ ...a })),
      );
    },

    async switchAccount(agentId, accountId) {
      await delay(randomLatency());
      const list = mockState[agentId];
      const from = list.find((a) => a.isCurrent);
      lastSwitch = { agentId, fromId: from?.id ?? '' };
      list.forEach((a) => (a.isCurrent = a.id === accountId));
    },

    async undoSwitchAccount(agentId) {
      await delay(200);
      if (!lastSwitch || lastSwitch.agentId !== agentId) return false;
      const list = mockState[agentId];
      list.forEach((a) => (a.isCurrent = a.id === lastSwitch!.fromId));
      lastSwitch = null;
      return true;
    },

    async addApiKeyAccount(agentId, key, label, envKey) {
      await delay(randomLatency());
      void envKey;
      const masked =
        key.length > 7
          ? `${key.slice(0, 3)}-••••${key.slice(-4)}`
          : '••••';
      const acc: Account = {
        id: `${agentId}-acc-${Date.now()}`,
        agentId,
        kind: 'apikey',
        label: label?.trim() || `${masked} (API Key)`,
        subscription: 'API Key',
        isCurrent: false,
        tokenValid: true,
        identityLabel: label?.trim() || masked,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      };
      mockState[agentId].push(acc);
      return { ...acc };
    },

    async updateApiKeyAccount(agentId, accountId, opts) {
      await delay(randomLatency());
      const list = mockState[agentId] ?? [];
      const acc = list.find((a) => a.id === accountId);
      if (!acc) throw new Error(`account not found: ${accountId}`);
      if (acc.kind !== 'apikey') throw new Error('only API Key accounts can be edited');
      const nextLabel = opts.label?.trim();
      if (nextLabel) {
        acc.label = nextLabel;
        acc.identityLabel = nextLabel;
      }
      if (opts.key?.trim()) {
        const key = opts.key.trim();
        const masked =
          key.length > 7
            ? `${key.slice(0, 3)}-••••${key.slice(-4)}`
            : '••••';
        if (!nextLabel) {
          acc.label = `${masked} (API Key)`;
        }
        acc.tokenValid = true;
      }
      if (!nextLabel && !opts.key?.trim()) {
        throw new Error('label or key is required');
      }
      acc.updatedAt = new Date().toISOString();
      return { ...acc };
    },

    async importCurrentLogin(agentId) {
      await delay(randomLatency());
      const acc: Account = {
        id: `${agentId}-acc-${Date.now()}`,
        agentId,
        kind: 'oauth',
        label: 'imported@gmail.com',
        email: 'imported@gmail.com',
        subscription: agentId === 'codex' ? 'ChatGPT Plus' : 'Claude Pro',
        isCurrent: false,
        tokenValid: true,
        tokenRemainingSec: 8 * 3600,
        quota5hPct: 5,
        lastUsedAt: new Date().toISOString(),
      };
      mockState[agentId].push(acc);
      return { ...acc };
    },

    async oauthSupported() {
      return true;
    },

    async startOAuth(agentId) {
      await delay(50);
      return {
        state: `mock-${agentId}-${Date.now()}`,
        authorizeUrl: 'http://127.0.0.1:34567/callback?code=mock-code&state=mock',
        redirectUri: 'http://127.0.0.1:34567/callback',
        agentId,
        browserOpened: false,
      };
    },

    async waitOAuth(state) {
      await delay(100);
      return {
        state,
        agentId: 'claude' as AgentId,
        status: 'callbackReceived' as const,
        error: null,
      };
    },

    async finishOAuth(state) {
      void state;
      return this.completeOAuth('claude');
    },

    async completeOAuth(agentId) {
      await delay(400);
      const email = `user${Math.floor(Math.random() * 900 + 100)}@gmail.com`;
      const acc: Account = {
        id: `${agentId}-acc-${Date.now()}`,
        agentId,
        kind: 'oauth',
        label: email,
        email,
        subscription:
          agentId === 'codex' ? 'ChatGPT Plus' : agentId === 'grok' ? 'SuperGrok' : 'Claude Pro',
        isCurrent: false,
        tokenValid: true,
        tokenRemainingSec: 30 * 24 * 3600,
        quota5hPct: 0,
        quota7dPct: 0,
        quotaResetIn: '5h00m 后重置',
        lastUsedAt: new Date().toISOString(),
      };
      mockState[agentId].push(acc);
      return { ...acc };
    },

    async deleteAccount(agentId, accountId) {
      await delay(randomLatency());
      mockState[agentId] = mockState[agentId].filter((a) => a.id !== accountId);
    },

    async refreshToken(agentId, accountId) {
      await delay(randomLatency());
      const acc = mockState[agentId].find((a) => a.id === accountId);
      if (acc) {
        acc.tokenValid = true;
        acc.tokenRemainingSec = 30 * 24 * 3600;
      }
    },
  };
}
