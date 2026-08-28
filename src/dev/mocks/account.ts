import type { AccountPort } from '@/lib/backend/contracts';
import { wrapBareAccount } from '@/lib/backend/contracts/account-map';
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

type MockOAuthSession = {
  agentId: AgentId;
  providerKey: string | null;
  flow: 'pkce' | 'device';
  devicePolls?: number;
};

const oauthSessions = new Map<string, MockOAuthSession>();

function requireOAuthSession(state: string): MockOAuthSession {
  const session = oauthSessions.get(state);
  if (!session) {
    throw new Error(`unknown oauth state: ${state}`);
  }
  return session;
}

/** Clears browser-mock account-pool state so each backend factory starts clean. */
export function resetMockAccounts(): void {
  (Object.keys(mockState) as AgentId[]).forEach((agentId) => {
    mockState[agentId].length = 0;
  });
  lastSwitch = null;
  oauthSessions.clear();
}

/** Synchronous test-only insertion used by ConnectFlow / adapter fixtures. */
export function upsertMockAccount(account: Account): Account {
  const list = mockState[account.agentId] ?? (mockState[account.agentId] = []);
  const index = list.findIndex((item) => item.id === account.id);
  if (account.isCurrent) {
    list.forEach((item) => {
      item.isCurrent = false;
    });
  }
  if (index >= 0) {
    list[index] = { ...account };
  } else {
    list.push({ ...account });
  }
  return { ...(index >= 0 ? list[index] : list[list.length - 1]) };
}

/** Read-only lookup used by browser-only compatibility previews. */
export function getMockAccountById(accountId: string): Account | undefined {
  const found = (Object.keys(mockState) as AgentId[])
    .flatMap((agentId) => mockState[agentId] ?? [])
    .find((account) => account.id === accountId);
  return found ? { ...found } : undefined;
}

/** Snapshot of all mock accounts (ticket wallet aggregation). */
export function listMockAccounts(): Account[] {
  return (Object.keys(mockState) as AgentId[]).flatMap((agentId) =>
    (mockState[agentId] ?? []).map((account) => ({ ...account })),
  );
}

export function createMockAccountPort(): AccountPort {
  return {
    async listAccounts(agentId) {
      await delay(randomLatency());
      if (agentId) {
        return (mockState[agentId] ?? []).map((a) => wrapBareAccount({ ...a }));
      }
      return (Object.keys(mockState) as AgentId[]).flatMap((id) =>
        (mockState[id] ?? []).map((a) => wrapBareAccount({ ...a })),
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
          health: current.authHealth ?? 'configured',
          source: 'mock-live-config',
          revision: current.updatedAt ?? null,
        };
      }
      if (current?.kind === 'oauth') {
        return {
          agentId,
          kind: 'oauth',
          summary: 'OAuth credentials present in mock live config',
          hasCredentials: true,
          health: current.authHealth ?? (current.refreshable ? 'renewable' : 'unknown'),
          source: 'mock-live-config',
          revision: current.updatedAt ?? null,
        };
      }
      return {
        agentId,
        kind: null,
        summary: 'no live credentials detected',
        hasCredentials: false,
        health: 'missing',
        source: 'mock-live-config',
        revision: null,
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

    async addApiKeyAccount(agentId, key, label, envKey, productMarker) {
      await delay(randomLatency());
      void envKey;
      void productMarker;
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
        authHealth: 'configured',
        identityLabel: label?.trim() || masked,
        secretTail: key.trim().length >= 8 ? `**${key.trim().slice(-4)}` : undefined,
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
        acc.secretTail = key.length >= 8 ? `**${key.slice(-4)}` : undefined;
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
        authHealth: 'renewable',
        refreshable: true,
        refreshTokenPreview: 'rt--••••JF6Q',
        secretTail: '**JF6Q',
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
            description: '浏览器登录，写入 Pi 的登录列表',
            flow: 'pkce' as const,
            authJsonKey: 'anthropic',
          },
          {
            id: 'openai-codex',
            agentId: 'pi',
            label: 'ChatGPT Plus/Pro (Codex)',
            description: '浏览器登录，写入 Pi 的登录列表',
            flow: 'pkce' as const,
            authJsonKey: 'openai-codex',
          },
          {
            id: 'xai',
            agentId: 'pi',
            label: 'xAI (Grok 订阅)',
            description: '设备码登录，写入 Pi 的登录列表',
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
            description: '浏览器登录',
            flow: 'pkce' as const,
          },
        ];
      }
      return [];
    },

    async startOAuth(agentId, _openBrowser, providerKey) {
      await delay(50);
      const state = `mock-${agentId}-${Date.now()}`;
      oauthSessions.set(state, {
        agentId,
        providerKey: providerKey ?? null,
        flow: 'pkce',
      });
      return {
        state,
        authorizeUrl: 'http://127.0.0.1:34567/callback?code=mock-code&state=mock',
        redirectUri: 'http://127.0.0.1:34567/callback',
        agentId,
        providerKey: providerKey ?? null,
        // Mock cannot open a system browser; keep the wait page in view.
        browserOpened: true,
        expiresInSecs: 900,
      };
    },

    async waitOAuth(state) {
      await delay(400);
      const session = requireOAuthSession(state);
      return {
        state,
        agentId: session.agentId,
        status: 'callbackReceived' as const,
        error: null,
      };
    },

    async finishOAuth(state) {
      const session = requireOAuthSession(state);
      return this.completeOAuth(session.agentId, session.providerKey);
    },

    async cancelOAuth(state) {
      oauthSessions.delete(state);
    },

    async startDeviceOAuth(agentId, providerKey) {
      await delay(50);
      const state = `mock-dev-${Date.now()}`;
      oauthSessions.set(state, {
        agentId,
        providerKey,
        flow: 'device',
      });
      return {
        state,
        agentId,
        providerKey,
        userCode: 'ABCD-EFGH',
        verificationUri: 'https://auth.x.ai/device',
        verificationUriComplete: 'https://auth.x.ai/device?user_code=ABCD-EFGH',
        intervalSecs: 6,
        expiresInSecs: 120,
      };
    },

    async pollDeviceOAuth(state) {
      await delay(80);
      const session = requireOAuthSession(state);
      session.devicePolls = (session.devicePolls ?? 0) + 1;
      if (session.flow === 'device' && session.devicePolls < 2) {
        return { state, status: 'pending' as const, error: null };
      }
      return { state, status: 'complete' as const, error: null };
    },

    async finishDeviceOAuth(state) {
      const session = requireOAuthSession(state);
      return this.completeOAuth(session.agentId, session.providerKey);
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
        authHealth: 'renewable',
        refreshable: true,
        refreshTokenPreview: 'rt--••••JF6Q',
        secretTail: '**JF6Q',
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
