import type { AccountPort } from '@/lib/backend/contracts';
import { delay, randomLatency } from '@/dev/mocks/delay';
import type { Account, AgentId } from '@/lib/types';
import { moveMockAccountToTrash } from './trash';

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

    async probeLiveAuth(agentId) {
      await delay(40);
      const current = (mockState[agentId] ?? []).find((a) => a.isCurrent);
      if (current?.kind === 'apikey') {
        return {
          agentId,
          kind: 'api_key',
          summary: 'API key present in mock live config',
          hasCredentials: true,
        };
      }
      if (current?.kind === 'oauth') {
        return {
          agentId,
          kind: 'oauth',
          summary: 'OAuth credentials present in mock live config',
          hasCredentials: true,
        };
      }
      return {
        agentId,
        kind: null,
        summary: 'no live credentials detected',
        hasCredentials: false,
      };
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

    async oauthSupported(agentId) {
      return agentId === 'claude' || agentId === 'codex' || agentId === 'grok' || agentId === 'pi';
    },

    async listOAuthOptions(agentId) {
      if (agentId === 'pi') {
        return [
          {
            id: 'anthropic',
            agentId: 'pi',
            label: 'Claude Pro/Max',
            description: '写入 Pi auth.json → anthropic',
            flow: 'pkce' as const,
            authJsonKey: 'anthropic',
          },
          {
            id: 'openai-codex',
            agentId: 'pi',
            label: 'ChatGPT Plus/Pro (Codex)',
            description: '写入 Pi auth.json → openai-codex',
            flow: 'pkce' as const,
            authJsonKey: 'openai-codex',
          },
          {
            id: 'xai',
            agentId: 'pi',
            label: 'xAI (Grok 订阅)',
            description: '设备码登录 → Pi auth.json → xai',
            flow: 'deviceCode' as const,
            authJsonKey: 'xai',
          },
        ];
      }
      if (agentId === 'claude' || agentId === 'codex' || agentId === 'grok') {
        return [
          {
            id: agentId,
            agentId,
            label: agentId,
            description: 'OAuth',
            flow: 'pkce' as const,
          },
        ];
      }
      return [];
    },

    async startOAuth(agentId, _openBrowser, providerKey) {
      await delay(50);
      return {
        state: `mock-${agentId}-${Date.now()}`,
        authorizeUrl: 'http://127.0.0.1:34567/callback?code=mock-code&state=mock',
        redirectUri: 'http://127.0.0.1:34567/callback',
        agentId,
        providerKey: providerKey ?? null,
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

    async startDeviceOAuth(agentId, providerKey) {
      await delay(50);
      return {
        state: `mock-dev-${Date.now()}`,
        agentId,
        providerKey,
        userCode: 'ABCD-EFGH',
        verificationUri: 'https://auth.x.ai/device',
        verificationUriComplete: 'https://auth.x.ai/device?user_code=ABCD-EFGH',
        intervalSecs: 1,
        expiresInSecs: 120,
      };
    },

    async pollDeviceOAuth(state) {
      await delay(80);
      return { state, status: 'complete' as const, error: null };
    },

    async finishDeviceOAuth(state) {
      void state;
      return this.completeOAuth('pi', 'xai');
    },

    async completeOAuth(agentId, providerKey) {
      await delay(400);
      const email = `user${Math.floor(Math.random() * 900 + 100)}@gmail.com`;
      const provider = providerKey ? String(providerKey) : undefined;
      const label =
        agentId === 'pi' && provider ? `pi:${provider} · ${email}` : email;
      const acc: Account = {
        id: `${agentId}-acc-${Date.now()}`,
        agentId,
        kind: 'oauth',
        label,
        email,
        identityLabel: email,
        subscription:
          agentId === 'codex' || provider === 'openai-codex'
            ? 'ChatGPT Plus'
            : agentId === 'grok' || provider === 'xai'
              ? 'SuperGrok'
              : 'Claude Pro',
        isCurrent: false,
        tokenValid: true,
        tokenRemainingSec: 30 * 24 * 3600,
        quota5hPct: 0,
        quota7dPct: 0,
        quotaResetIn: '5h00m 后重置',
        lastUsedAt: new Date().toISOString(),
        source: agentId === 'pi' ? 'oauth_pkce' : 'oauth_pkce',
      };
      mockState[agentId].push(acc);
      return { ...acc };
    },

    async deleteAccount(agentId, accountId) {
      await delay(randomLatency());
      const removed = mockState[agentId].find((account) => account.id === accountId);
      if (removed) moveMockAccountToTrash(removed);
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

    async refreshQuota(agentId, accountId) {
      await delay(randomLatency());
      const acc = mockState[agentId].find((a) => a.id === accountId);
      if (!acc) throw new Error('account not found');
      acc.quota5hPct = 12;
      acc.quota7dPct = 34;
      acc.quotaResetIn = '4h20m 后重置';
      return { ...acc };
    },
  };
}

export function restoreMockAccount(account: Account): void {
  const list = mockState[account.agentId];
  if (list.some((item) => item.id === account.id)) {
    throw new Error(`account already exists: ${account.id}`);
  }
  list.push({ ...account, isCurrent: false });
}
