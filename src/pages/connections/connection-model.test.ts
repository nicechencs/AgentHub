import { describe, expect, it } from 'vitest';
import { connectSourceKey, type ConnectionUsage, type ConnectionUsageMap } from '@/lib/connect-flow/types';
import type { Account, Provider } from '@/lib/types';
import {
  accountToEntry,
  authStatusOfAccount,
  countByKind,
  deleteConnectionDialogDescription,
  deleteConnectionToastDescription,
  filterConnectionEntries,
  beginExclusiveBusyIds,
  endExclusiveBusyIds,
  isCurrentSwitchPreviewRequest,
  isLiveAuthDiscoveryDeferred,
  liveApiKeyImportGate,
  liveAuthCoexistenceNotice,
  liveAuthDiscoveryKind,
  liveAuthImportGate,
  liveImportDialogMode,
  mergeConnectionEntries,
  providerToEntry,
} from './connection-model';

function acc(partial: Partial<Account> & Pick<Account, 'id' | 'kind' | 'label'>): Account {
  return {
    agentId: 'claude',
    isCurrent: false,
    tokenValid: true,
    ...partial,
  };
}

function prov(partial: Partial<Provider> & Pick<Provider, 'id' | 'name'>): Provider {
  return {
    agentId: 'claude',
    preset: 'custom',
    configText: JSON.stringify({
      env: {
        ANTHROPIC_BASE_URL: 'https://relay.example.com',
        ANTHROPIC_AUTH_TOKEN: '***',
      },
    }),
    configFormat: 'json',
    isCurrent: false,
    ...partial,
  };
}

describe('connection-model', () => {
  it('surfaces account email / subscription on oauth entries', () => {
    const entry = accountToEntry(
      acc({
        id: 'o1',
        kind: 'oauth',
        label: 'me@x.com',
        email: 'me@x.com',
        identityLabel: 'me@x.com',
        subscription: 'plus',
        isCurrent: false,
        source: 'oauth_pkce',
      }),
    );
    expect(entry.title).toBe('me@x.com');
    expect(entry.identityLabel).toBe('me@x.com');
    expect(entry.subscription).toBe('plus');
    expect(entry.subtitle).toContain('plus');
    expect(entry.subtitle).not.toContain('oauth_pkce');
  });

  it('treats oauth without remaining as valid, not none', () => {
    expect(
      authStatusOfAccount(
        acc({ id: 'o', kind: 'oauth', label: 'x', tokenValid: true }),
      ),
    ).toBe('valid');
    expect(
      authStatusOfAccount(
        acc({
          id: 'o2',
          kind: 'oauth',
          label: 'x',
          tokenValid: true,
          tokenRemainingSec: -10,
        }),
      ),
    ).toBe('expired');
    expect(
      authStatusOfAccount(
        acc({
          id: 'o3',
          kind: 'oauth',
          label: 'x',
          tokenValid: true,
          tokenRemainingSec: 2 * 3600,
        }),
      ),
    ).toBe('expiring');
    expect(
      authStatusOfAccount(
        acc({
          id: 'o4',
          kind: 'oauth',
          label: 'x',
          tokenValid: true,
          refreshable: true,
          tokenRemainingSec: -10,
        }),
      ),
    ).toBe('valid');
  });

  it('maps oauth / apikey; providers collapse into apikey kind', () => {
    expect(accountToEntry(acc({ id: 'a1', kind: 'oauth', label: 'me@x.com' })).kind).toBe(
      'oauth',
    );
    expect(accountToEntry(acc({ id: 'a2', kind: 'apikey', label: 'sk-…' })).kind).toBe(
      'apikey',
    );
    const p = providerToEntry(prov({ id: 'p1', name: 'relay' }));
    expect(p.kind).toBe('apikey');
    expect(p.source).toBe('provider');
    expect(p.endpointMode).toBe('custom');
  });

  it('marks official providers', () => {
    const p = providerToEntry(
      prov({
        id: 'p-off',
        name: 'Anthropic',
        official: true,
        configText: JSON.stringify({ env: {}, model: 'sonnet' }),
      }),
    );
    expect(p.endpointMode).toBe('official');
  });

  it('merges pools with current first then newer updatedAt', () => {
    const rows = mergeConnectionEntries(
      [
        acc({
          id: 'old',
          kind: 'oauth',
          label: 'old',
          updatedAt: '2026-01-01 00:00:00',
        }),
        acc({
          id: 'cur',
          kind: 'apikey',
          label: 'key',
          isCurrent: true,
          updatedAt: '2026-01-02 00:00:00',
        }),
      ],
      [
        prov({
          id: 'p-new',
          name: 'new-relay',
          updatedAt: '2026-06-01 00:00:00',
        }),
      ],
    );
    expect(rows.map((r) => r.id)).toEqual(['cur', 'p-new', 'old']);
    expect(rows[0]!.isCurrent).toBe(true);
  });

  it('filters and counts: provider rows count as apikey', () => {
    const rows = mergeConnectionEntries(
      [
        acc({ id: 'o', kind: 'oauth', label: 'o' }),
        acc({ id: 'k', kind: 'apikey', label: 'k' }),
      ],
      [prov({ id: 'p', name: 'p' })],
    );
    const counts = countByKind(rows);
    expect(counts).toEqual({ all: 3, oauth: 1, apikey: 2 });
    expect(filterConnectionEntries(rows, 'apikey')).toHaveLength(2);
    expect(filterConnectionEntries(rows, 'oauth')[0]!.id).toBe('o');
  });

  it('uses the same recoverable delete semantics for current accounts and providers', () => {
    for (const current of [true, false]) {
      const dialog = deleteConnectionDialogDescription({ isCurrent: current });
      const toast = deleteConnectionToastDescription({ isCurrent: current });
      expect(dialog).toContain('移入回收站');
      expect(dialog).toContain(current ? '当前连接可能仍继续生效' : '不会修改本机配置文件');
      expect(toast).toContain('已移入回收站');
      expect(toast).toContain(current ? '当前连接可能仍继续生效' : '本机配置未修改');
    }
  });

  it('only enables current-login import for credentialed OAuth/file-auth probes', () => {
    expect(liveAuthImportGate(undefined, true, 'claude')).toEqual({
      enabled: false,
      reason: '正在检测本机登录态…',
    });
    expect(
      liveAuthImportGate({ agentId: 'claude', kind: 'api_key', hasCredentials: true }, false, 'claude')
        .reason,
    ).toContain('API Key');
    expect(
      liveAuthImportGate(
        {
          agentId: 'claude',
          kind: 'api_key',
          hasCredentials: true,
          isAdapterProjection: true,
        },
        false,
        'claude',
      ),
    ).toEqual({
      enabled: false,
      reason: '当前是本机路由写进去的配置，不是一份新登录',
    });
    expect(
      liveAuthImportGate(
        {
          agentId: 'claude',
          kind: 'api_key',
          hasCredentials: true,
          alsoPresent: ['adapter_projection'],
        },
        false,
        'claude',
      ).enabled,
    ).toBe(false);
    expect(
      liveAuthImportGate(
        { agentId: 'claude', kind: 'desktop-login', hasCredentials: true },
        false,
        'claude',
      ).enabled,
    ).toBe(false);
    expect(
      liveAuthImportGate({ agentId: 'claude', kind: 'oauth', hasCredentials: false }, false, 'claude')
        .enabled,
    ).toBe(false);
    expect(
      liveAuthImportGate({ agentId: 'claude', kind: 'oauth', hasCredentials: true }, false, 'claude'),
    ).toEqual({
      enabled: true,
      reason: '',
    });
    expect(
      liveAuthImportGate(
        { agentId: 'claude', kind: 'file-auth.json', hasCredentials: true },
        false,
        'claude',
      ),
    ).toEqual({ enabled: true, reason: '' });
  });

  it('does not authorize an import while the selected agent has changed', () => {
    const previousAgentProbe = { agentId: 'claude' as const, kind: 'oauth', hasCredentials: true };

    expect(liveAuthImportGate(previousAgentProbe, false, 'codex')).toEqual({
      enabled: false,
      reason: '本机登录态正在切换，已禁用导入',
    });
    expect(liveApiKeyImportGate(previousAgentProbe, false, 'codex')).toEqual({
      enabled: false,
      reason: '本机认证方式正在切换，已禁用 API Key 导入',
    });
  });

  it('disables both import gates when live is a local-route projection', () => {
    const probe = {
      agentId: 'claude' as const,
      kind: 'api_key',
      hasCredentials: true,
      isAdapterProjection: true,
    };
    expect(liveAuthImportGate(probe, false, 'claude')).toEqual({
      enabled: false,
      reason: '当前是本机路由写进去的配置，不是一份新登录',
    });
    expect(liveApiKeyImportGate(probe, false, 'claude')).toEqual({
      enabled: false,
      reason: '当前是本机路由写进去的配置，不是一份新登录',
    });
  });

  describe('live-auth coexistence', () => {
    it('does not warn when a second credential family is absent', () => {
      expect(
        liveAuthCoexistenceNotice(
          {
            agentId: 'claude',
            kind: 'api_key',
            hasCredentials: true,
            alsoPresent: ['oauth', 'adapter_projection'],
            isAdapterProjection: true,
          },
          'claude',
        ),
      ).toBeNull();
      expect(
        liveAuthCoexistenceNotice(
          { agentId: 'claude', kind: 'oauth', hasCredentials: true, alsoPresent: [] },
          'claude',
        ),
      ).toBeNull();
      expect(
        liveAuthCoexistenceNotice(
          { agentId: 'claude', kind: 'oauth', hasCredentials: true },
          'claude',
        ),
      ).toBeNull();
      expect(
        liveAuthCoexistenceNotice(
          { agentId: 'claude', kind: 'oauth', hasCredentials: true, alsoPresent: ['nope'] },
          'claude',
        ),
      ).toBeNull();
      expect(liveAuthCoexistenceNotice(null, 'claude')).toBeNull();
    });

    it('warns about a second live credential family without changing import gates', () => {
      const claudeDual = {
        agentId: 'claude' as const,
        kind: 'api_key',
        hasCredentials: true,
        alsoPresent: ['oauth'],
      };
      expect(liveAuthCoexistenceNotice(claudeDual, 'claude')).toContain('Key');
      expect(liveAuthImportGate(claudeDual, false, 'claude')).toEqual({
        enabled: false,
        reason: '当前本机配置为 API Key，不是 OAuth 登录态',
      });
      expect(liveApiKeyImportGate(claudeDual, false, 'claude')).toEqual({
        enabled: true,
        reason: '',
      });

      const oauthAlsoApiKey = {
        agentId: 'claude' as const,
        kind: 'oauth',
        hasCredentials: true,
        alsoPresent: ['api_key'],
      };
      expect(liveAuthImportGate(oauthAlsoApiKey, false, 'claude')).toEqual({
        enabled: true,
        reason: '',
      });
      expect(liveApiKeyImportGate(oauthAlsoApiKey, false, 'claude')).toEqual({
        enabled: false,
        reason: '当前本机为 OAuth 登录态，请导入当前登录态',
      });
    });

    it('uses agent-specific copy for grok / kimi / codex and a generic cursor notice', () => {
      expect(
        liveAuthCoexistenceNotice(
          { agentId: 'grok', kind: 'api_key', hasCredentials: true, alsoPresent: ['oauth'] },
          'grok',
        ),
      ).toContain('grok login');
      expect(
        liveAuthCoexistenceNotice(
          { agentId: 'kimi', kind: 'api_key', hasCredentials: true, alsoPresent: ['oauth'] },
          'kimi',
        ),
      ).toMatch(/config\.toml|\/login/);
      expect(
        liveAuthCoexistenceNotice(
          { agentId: 'codex', kind: 'api_key', hasCredentials: true, alsoPresent: ['oauth'] },
          'codex',
        ),
      ).toMatch(/ChatGPT|TUI/);
      expect(
        liveAuthCoexistenceNotice(
          { agentId: 'cursor', kind: 'api_key', hasCredentials: true, alsoPresent: ['oauth'] },
          'cursor',
        ),
      ).toContain('本机同时有');
    });

    it('treats trimmed file-auth.json in alsoPresent as an oauth family', () => {
      expect(
        liveAuthCoexistenceNotice(
          {
            agentId: 'claude',
            kind: 'api_key',
            hasCredentials: true,
            alsoPresent: [' file-auth.json '],
          },
          'claude',
        ),
      ).toContain('Key');
    });

    it('still returns the pi notice for kind mixed without alsoPresent', () => {
      const piMixed = { agentId: 'pi' as const, kind: 'mixed', hasCredentials: true };
      expect(liveAuthCoexistenceNotice(piMixed, 'pi')).toMatch(/auth\.json|provider/);
      expect(liveAuthImportGate(piMixed, false, 'pi').enabled).toBe(false);
    });
  });

  it('only enables API Key import for credentialed API-key probes', () => {
    expect(liveApiKeyImportGate(undefined, true, 'claude')).toEqual({
      enabled: false,
      reason: '正在检测本机认证方式…',
    });
    expect(
      liveApiKeyImportGate({ agentId: 'claude', kind: 'oauth', hasCredentials: true }, false, 'claude'),
    ).toEqual({ enabled: false, reason: '当前本机为 OAuth 登录态，请导入当前登录态' });
    expect(
      liveApiKeyImportGate({ agentId: 'claude', kind: 'api_key', hasCredentials: false }, false, 'claude')
        .enabled,
    ).toBe(false);
    expect(
      liveApiKeyImportGate({ agentId: 'claude', kind: 'api_key', hasCredentials: true }, false, 'claude'),
    ).toEqual({
      enabled: true,
      reason: '',
    });
  });

  it('does not report a new login while the connection pool is still loading', () => {
    const probe = { kind: 'oauth', hasCredentials: true };
    expect(liveAuthDiscoveryKind({
      poolState: 'loading',
      probe,
      accounts: [],
      providers: [],
    })).toBeNull();
    expect(liveAuthDiscoveryKind({
      poolState: 'idle',
      probe,
      accounts: [],
      providers: [],
    })).toBeNull();
    expect(liveAuthDiscoveryKind({
      poolState: 'ready',
      probe,
      accounts: [],
      providers: [],
    })).toBe('account');
    expect(liveAuthDiscoveryKind({
      poolState: 'ready',
      probe,
      accounts: [{ kind: 'oauth' }],
      providers: [],
    })).toBeNull();
    expect(liveAuthDiscoveryKind({
      poolState: 'ready',
      probe: { kind: 'api_key', hasCredentials: true },
      accounts: [],
      providers: [],
    })).toBe('provider');
    expect(liveAuthDiscoveryKind({
      poolState: 'error',
      probe,
      accounts: [],
      providers: [],
    })).toBeNull();
    expect(liveAuthDiscoveryKind({
      poolState: 'partial',
      probe,
      accounts: [],
      providers: [],
      accountsFailed: true,
    })).toBeNull();
    expect(isLiveAuthDiscoveryDeferred({
      poolState: 'partial',
      probe,
      accountsFailed: true,
    })).toBe(true);
    expect(isLiveAuthDiscoveryDeferred({
      poolState: 'ready',
      probe,
      accountsFailed: false,
    })).toBe(false);
    expect(liveAuthDiscoveryKind({
      poolState: 'ready',
      probe,
      accounts: [],
      providers: [],
      accountsFailed: false,
    })).toBe('account');
  });

  it('ignores a stale switch-preview result after the selected agent changes', () => {
    expect(isCurrentSwitchPreviewRequest('claude', 'codex', 1, 2)).toBe(false);
    expect(isCurrentSwitchPreviewRequest('claude', 'claude', 2, 2)).toBe(true);
  });

  it('serializes recycle-bin mutations so a later finish cannot clear a newer busy id', () => {
    const empty = new Set<string>();
    const first = beginExclusiveBusyIds(empty, 'trash-1');
    expect(first).toEqual(new Set(['trash-1']));
    expect(beginExclusiveBusyIds(first!, 'trash-2')).toBeNull();
    expect(beginExclusiveBusyIds(first!, 'trash-1')).toBeNull();

    const released = endExclusiveBusyIds(first!, 'trash-1');
    expect(released.size).toBe(0);
    expect(endExclusiveBusyIds(first!, 'trash-2')).toEqual(first);
  });

  it('fills usage from usageMap by isomorphic account:/provider: keys', () => {
    const knownUsed: ConnectionUsage = {
      status: 'known',
      agents: [
        { agentId: 'claude', via: 'direct' },
        { agentId: 'codex', via: 'adapter' },
      ],
    };
    const knownEmpty: ConnectionUsage = { status: 'known', agents: [] };
    const incomplete: ConnectionUsage = { status: 'incomplete', agents: [] };
    const usageMap: ConnectionUsageMap = new Map([
      [connectSourceKey({ kind: 'account', id: 'used' }), knownUsed],
      [connectSourceKey({ kind: 'account', id: 'idle' }), knownEmpty],
      [connectSourceKey({ kind: 'provider', id: 'unk' }), incomplete],
    ]);

    const rows = mergeConnectionEntries(
      [
        acc({ id: 'used', kind: 'oauth', label: 'used' }),
        acc({ id: 'idle', kind: 'oauth', label: 'idle' }),
        acc({ id: 'bare', kind: 'oauth', label: 'bare' }),
      ],
      [prov({ id: 'unk', name: 'unk' }), prov({ id: 'other', name: 'other' })],
      usageMap,
    );

    expect(rows.find((r) => r.id === 'used')?.usage).toEqual(knownUsed);
    expect(rows.find((r) => r.id === 'idle')?.usage).toEqual(knownEmpty);
    expect(rows.find((r) => r.id === 'unk')?.usage).toEqual(incomplete);
    expect(rows.find((r) => r.id === 'bare')?.usage).toBeUndefined();
    expect(rows.find((r) => r.id === 'other')?.usage).toBeUndefined();
  });

  it('is equivalent to the current merge when usageMap is omitted', () => {
    const accounts = [
      acc({
        id: 'cur',
        kind: 'apikey',
        label: 'key',
        isCurrent: true,
        updatedAt: '2026-01-02 00:00:00',
      }),
      acc({ id: 'old', kind: 'oauth', label: 'old', updatedAt: '2026-01-01 00:00:00' }),
    ];
    const providers = [prov({ id: 'p-new', name: 'new-relay', updatedAt: '2026-06-01 00:00:00' })];

    const without = mergeConnectionEntries(accounts, providers);
    const emptyMap = mergeConnectionEntries(accounts, providers, new Map());

    expect(without.map((r) => r.id)).toEqual(['cur', 'p-new', 'old']);
    expect(emptyMap.map((r) => r.id)).toEqual(without.map((r) => r.id));
    expect(without.every((r) => r.usage === undefined)).toBe(true);
    expect(emptyMap.every((r) => r.usage === undefined)).toBe(true);
    expect(without.map(({ usage: _u, ...rest }) => rest)).toEqual(
      emptyMap.map(({ usage: _u, ...rest }) => rest),
    );
  });

  it('does not cross-match the same id across account and provider kinds', () => {
    const usageMap: ConnectionUsageMap = new Map([
      [
        connectSourceKey({ kind: 'account', id: 'same' }),
        { status: 'known', agents: [{ agentId: 'kimi', via: 'direct' }] },
      ],
      [connectSourceKey({ kind: 'provider', id: 'same' }), { status: 'incomplete', agents: [] }],
    ]);

    const rows = mergeConnectionEntries(
      [acc({ id: 'same', kind: 'oauth', label: 'acc' })],
      [prov({ id: 'same', name: 'prov' })],
      usageMap,
    );

    expect(rows.find((r) => r.source === 'account')?.usage).toEqual({
      status: 'known',
      agents: [{ agentId: 'kimi', via: 'direct' }],
    });
    expect(rows.find((r) => r.source === 'provider')?.usage).toEqual({
      status: 'incomplete',
      agents: [],
    });
    expect(accountToEntry(acc({ id: 'same', kind: 'oauth', label: 'acc' })).usage).toBeUndefined();
    expect(providerToEntry(prov({ id: 'same', name: 'prov' })).usage).toBeUndefined();
  });
});

describe('liveImportDialogMode', () => {
  it('picks the api-key variant only for probed api key kinds', () => {
    expect(liveImportDialogMode({ agentId: 'pi', kind: 'api_key', hasCredentials: true })).toBe('api-key');
    expect(liveImportDialogMode({ agentId: 'pi', kind: 'API-KEY', hasCredentials: true })).toBe('api-key');
    expect(liveImportDialogMode({ agentId: 'pi', kind: 'apikey' })).toBe('api-key');
    expect(liveImportDialogMode({ agentId: 'pi', kind: 'oauth', hasCredentials: true })).toBe('login');
    expect(liveImportDialogMode({ agentId: 'pi', kind: 'file-auth.json', hasCredentials: true })).toBe('login');
    expect(liveImportDialogMode({ agentId: 'pi', kind: 'desktop-login' })).toBe('login');
    expect(liveImportDialogMode(null)).toBe('login');
    expect(liveImportDialogMode(undefined)).toBe('login');
  });
});
