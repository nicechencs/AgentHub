import { describe, expect, it } from 'vitest';
import {
  agentCanReceiveTokenImport,
  agentMatchesTokenSurface,
  agentWritesLocalTokenKind,
  findReusablePiTokenProvider,
  eligibleAgentsForTokenImport,
  isTokenImportAgentVisible,
  resolveTokenImportProfile,
  tokenImportAgentChoice,
  tokenImportApiKeyDraft,
  tokenImportConnectionsUrl,
  tokenImportGate,
  tokenImportSurface,
} from './token-import-model';
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';
import type { Provider } from '@/lib/types';
import type { LocalTokenRow } from './tokens-model';

function row(
  partial: Partial<LocalTokenRow> = {},
): Pick<LocalTokenRow, 'kind' | 'token' | 'profileId' | 'unavailable'> {
  return {
    kind: 'messages',
    token: 'ahb_secret',
    profileId: 'profile-1',
    unavailable: false,
    ...partial,
  };
}

const PI_KEY_HASH = 'c63ba3b89ec037c1719d24cb78471adc9e4136d864c539892f01eb3c2eab4692';

function piProvider(input: {
  id: string;
  baseUrl?: string;
  piApi?: string;
  model?: string;
  secretHash?: string;
  updatedAt?: string;
}): Provider {
  return {
    id: input.id,
    agentId: 'pi',
    name: 'custom',
    preset: 'custom',
    configText: JSON.stringify({
      models: {
        providers: {
          custom: {
            baseUrl: input.baseUrl ?? 'http://127.0.0.1:17034/v1',
            api: input.piApi ?? 'openai-responses',
            models: [{ id: input.model ?? 'gpt-5' }],
          },
        },
      },
    }),
    configFormat: 'json',
    isCurrent: false,
    secretHash: input.secretHash ?? PI_KEY_HASH,
    updatedAt: input.updatedAt,
  };
}

describe('tokenImportSurface', () => {
  it('maps LocalTokenRow.kind onto wire conversation surfaces', () => {
    expect(tokenImportSurface('messages')).toBe('messages');
    expect(tokenImportSurface('responses_codex')).toBe('responses');
    expect(tokenImportSurface('responses_grok')).toBe('responses');
    expect(tokenImportSurface('chat_completions')).toBe('chat_completions');
  });
});

describe('agentMatchesTokenSurface', () => {
  it('matches Agents that speak the token surface', () => {
    expect(agentMatchesTokenSurface('claude', 'messages')).toBe(true);
    expect(agentMatchesTokenSurface('codex', 'messages')).toBe(false);
    expect(agentMatchesTokenSurface('codex', 'responses_codex')).toBe(true);
    expect(agentMatchesTokenSurface('grok', 'responses_grok')).toBe(true);
    expect(agentMatchesTokenSurface('workbuddy', 'chat_completions')).toBe(true);
    expect(agentMatchesTokenSurface('workbuddy', 'messages')).toBe(false);
  });

  it('lets multi-surface Agents match any of the three kinds', () => {
    for (const agent of ['pi', 'zcode', 'kimi', 'dsh'] as const) {
      expect(agentMatchesTokenSurface(agent, 'messages')).toBe(true);
      expect(agentMatchesTokenSurface(agent, 'responses_codex')).toBe(true);
      expect(agentMatchesTokenSurface(agent, 'chat_completions')).toBe(true);
    }
  });

  it('rejects Cursor Agent (no public HTTP surface)', () => {
    expect(agentMatchesTokenSurface('cursor', 'messages')).toBe(false);
    expect(agentMatchesTokenSurface('cursor', 'responses_codex')).toBe(false);
    expect(agentMatchesTokenSurface('cursor', 'chat_completions')).toBe(false);
  });
});

describe('agentWritesLocalTokenKind', () => {
  it('maps loopback writers and rejects Agents without a generated local-gateway write', () => {
    expect(agentWritesLocalTokenKind('claude')).toBe('messages');
    expect(agentWritesLocalTokenKind('codex')).toBe('responses_codex');
    expect(agentWritesLocalTokenKind('grok')).toBe('responses_grok');
    expect(agentWritesLocalTokenKind('kimi')).toBe('chat_completions');
    expect(agentWritesLocalTokenKind('dsh')).toBe('chat_completions');
    expect(agentWritesLocalTokenKind('pi')).toBeNull();
    expect(agentWritesLocalTokenKind('workbuddy')).toBeNull();
    expect(agentWritesLocalTokenKind('zcode')).toBeNull();
    expect(agentWritesLocalTokenKind('cursor')).toBeNull();
  });
});

describe('agentCanReceiveTokenImport', () => {
  it('enables Agents that speak this endpoint', () => {
    expect(agentCanReceiveTokenImport('claude', 'messages')).toBe(true);
    expect(agentCanReceiveTokenImport('kimi', 'messages')).toBe(true);
    expect(agentCanReceiveTokenImport('pi', 'chat_completions')).toBe(true);
    expect(agentCanReceiveTokenImport('workbuddy', 'chat_completions')).toBe(true);
    expect(agentCanReceiveTokenImport('codex', 'messages')).toBe(false);
    expect(agentCanReceiveTokenImport('cursor', 'chat_completions')).toBe(false);
    expect(agentCanReceiveTokenImport('grok', 'responses_codex')).toBe(false);
  });
});

describe('isTokenImportAgentVisible', () => {
  it('requires installed && !hidden', () => {
    expect(isTokenImportAgentVisible({ installed: true, hidden: false })).toBe(true);
    expect(isTokenImportAgentVisible({ installed: true, hidden: true })).toBe(false);
    expect(isTokenImportAgentVisible({ installed: false, hidden: false })).toBe(false);
    expect(isTokenImportAgentVisible(null)).toBe(false);
  });
});

describe('eligibleAgentsForTokenImport', () => {
  it('keeps installed order and drops Agents that cannot receive this loopback', () => {
    const agents = eligibleAgentsForTokenImport({
      kind: 'messages',
      installedIds: ['codex', 'claude', 'pi', 'cursor'],
      agentName: (id) => id.toUpperCase(),
    });
    expect(agents.map((a) => a.id)).toEqual(['claude', 'pi']);
    expect(agents[0]?.name).toBe('CLAUDE');
  });

  it('builds installed ids from statuses when installedIds omitted', () => {
    const agents = eligibleAgentsForTokenImport({
      kind: 'responses_codex',
      statuses: [
        { agentId: 'claude', installed: true, hidden: false },
        { agentId: 'codex', installed: true, hidden: false },
        { agentId: 'grok', installed: true, hidden: true },
        { agentId: 'pi', installed: false, hidden: false },
      ],
    });
    expect(agents.map((a) => a.id)).toEqual(['codex']);
  });

  it('returns empty for chat_completions when only Claude/Codex/Grok are installed', () => {
    expect(eligibleAgentsForTokenImport({
      kind: 'chat_completions',
      installedIds: ['claude', 'codex', 'grok'],
    })).toEqual([]);
  });

  it('includes Kimi and DSH for chat_completions when installed', () => {
    expect(eligibleAgentsForTokenImport({
      kind: 'chat_completions',
      installedIds: ['claude', 'workbuddy', 'kimi', 'dsh'],
    }).map((a) => a.id)).toEqual(['workbuddy', 'kimi', 'dsh']);
  });
});

describe('tokenImportAgentChoice', () => {
  it('enables a matching writer and explains the others', () => {
    expect(tokenImportAgentChoice('messages', { id: 'claude', name: 'Claude' })).toEqual({
      id: 'claude',
      name: 'Claude',
      enabled: true,
      reason: null,
    });
    expect(tokenImportAgentChoice('messages', { id: 'pi', name: 'Pi' }).enabled).toBe(true);
    expect(tokenImportAgentChoice('messages', { id: 'codex', name: 'Codex' }).reason).toBe('端点不匹配');
    expect(tokenImportAgentChoice('messages', { id: 'cursor', name: 'Cursor' }).reason).toBe('没有可用端点');
  });
});

describe('tokenImportGate', () => {
  const installed = [
    { id: 'claude' as const, name: 'Claude' },
    { id: 'codex' as const, name: 'Codex' },
    { id: 'pi' as const, name: 'Pi' },
  ];

  it('opens the menu when key and any installed Agent are ready', () => {
    const gate = tokenImportGate(row({ profileId: null }), installed);
    expect(gate.enabled).toBe(true);
    expect(gate.reason).toBeNull();
    expect(gate.agents.map((a) => a.id)).toEqual(['claude', 'codex', 'pi']);
    expect(gate.agents.find((a) => a.id === 'claude')?.enabled).toBe(true);
    expect(gate.agents.find((a) => a.id === 'codex')?.enabled).toBe(false);
    expect(gate.agents.find((a) => a.id === 'pi')?.enabled).toBe(true);
  });

  it('still opens when nobody can receive this endpoint (items explain why)', () => {
    const gate = tokenImportGate(
      row({ kind: 'chat_completions' }),
      [{ id: 'claude', name: 'Claude' }, { id: 'codex', name: 'Codex' }],
    );
    expect(gate.enabled).toBe(true);
    expect(gate.agents.every((agent) => !agent.enabled)).toBe(true);
    expect(gate.agents[0]?.reason).toBe('端点不匹配');
  });

  it('disables without a key or installed Agent', () => {
    expect(tokenImportGate(row({ token: null }), installed).reason).toBe('先有入口 Key 才能导入');
    expect(tokenImportGate(row({ unavailable: true }), installed).reason).toBe('状态不可用');
    expect(tokenImportGate(row(), []).reason).toBe('先安装 Agent');
  });
});

describe('tokenImportApiKeyDraft', () => {
  it('fills origin + key for a matching Agent and skips the URL when the port is pending', () => {
    expect(tokenImportApiKeyDraft({
      kind: 'messages',
      token: 'ahb_secret',
      path: '/v1/messages',
      endpoint: '127.0.0.1:17034',
      listedModels: ['claude-sonnet-4'],
    }, 'claude')).toEqual({
      baseUrl: 'http://127.0.0.1:17034',
      apiKey: 'ahb_secret',
      model: 'claude-sonnet-4',
    });
    expect(tokenImportApiKeyDraft({
      kind: 'responses_grok',
      token: 'ahb_secret',
      path: '/v1/responses',
      endpoint: null,
      listedModels: [],
    }, 'grok')).toEqual({
      apiKey: 'ahb_secret',
      apiBackend: 'responses',
    });
    expect(tokenImportApiKeyDraft({
      kind: 'messages',
      token: 'ahb_secret',
      path: '/v1/messages',
      endpoint: null,
      listedModels: [],
    }, 'pi')).toEqual({
      apiKey: 'ahb_secret',
      piApi: 'anthropic-messages',
    });
    expect(tokenImportApiKeyDraft({
      kind: 'responses_codex',
      token: 'ahb_secret',
      path: '/v1/responses',
      endpoint: '127.0.0.1:17034',
      listedModels: ['gpt-5.1-codex'],
    }, 'pi')).toEqual({
      baseUrl: 'http://127.0.0.1:17034/v1',
      apiKey: 'ahb_secret',
      model: 'gpt-5.1-codex',
      piApi: 'openai-responses',
    });
    expect(tokenImportApiKeyDraft({
      kind: 'chat_completions',
      token: 'ahb_secret',
      path: '/v1/chat/completions',
      endpoint: '127.0.0.1:17034',
      listedModels: [],
    }, 'pi')).toEqual({
      baseUrl: 'http://127.0.0.1:17034',
      apiKey: 'ahb_secret',
      piApi: 'openai-completions',
    });
    expect(tokenImportApiKeyDraft({
      kind: 'responses_codex',
      token: 'ahb_secret',
      path: '/v1/responses',
      endpoint: '127.0.0.1:17034',
      listedModels: ['gpt-5.1-codex'],
    }, 'codex')).toEqual({
      baseUrl: 'http://127.0.0.1:17034',
      apiKey: 'ahb_secret',
      model: 'gpt-5.1-codex',
    });
    expect(tokenImportApiKeyDraft({
      kind: 'responses_grok',
      token: 'ahb_secret',
      path: '/v1/responses',
      endpoint: null,
      listedModels: [],
    }, 'pi')).toBeNull();
    expect(tokenImportApiKeyDraft({
      kind: 'messages',
      token: 'ahb_secret',
      path: '/v1/messages',
      endpoint: '127.0.0.1:17034',
      listedModels: [],
    }, 'codex')).toBeNull();
  });

  it('opens Connections add-key without resume so the user stays on the settings panel', () => {
    expect(tokenImportConnectionsUrl('claude')).toBe(
      '/connections?agent=claude&mode=providers&intent=add-key',
    );
  });
});

describe('findReusablePiTokenProvider', () => {
  const draft = {
    baseUrl: 'http://127.0.0.1:17034/v1/',
    apiKey: 'ahb_secret',
    model: 'gpt-5',
    piApi: 'openai-responses' as const,
  };

  it('selects the newest exact match and rejects model, API, URL, or hash changes', async () => {
    const older = piProvider({ id: 'pi-old', updatedAt: '2026-10-01T01:00:00.000Z' });
    const newer = piProvider({ id: 'pi-new', updatedAt: '2026-10-02T01:00:00.000Z' });
    expect(await findReusablePiTokenProvider({ draft, providers: [older, newer] })).toMatchObject({
      id: 'pi-new',
    });
    expect(await findReusablePiTokenProvider({
      draft: { ...draft, model: 'gpt-5-mini' },
      providers: [newer],
    })).toBeNull();
    expect(await findReusablePiTokenProvider({
      draft: { ...draft, piApi: 'openai-completions' },
      providers: [newer],
    })).toBeNull();
    expect(await findReusablePiTokenProvider({
      draft: { ...draft, baseUrl: 'http://127.0.0.1:17035/v1' },
      providers: [newer],
    })).toBeNull();
    expect(await findReusablePiTokenProvider({
      draft: { ...draft, apiKey: 'ahb_other' },
      providers: [newer],
    })).toBeNull();
    expect(await findReusablePiTokenProvider({
      draft: { ...draft, model: undefined },
      providers: [newer],
    })).toBeNull();

    const tenant = piProvider({
      id: 'pi-tenant',
      baseUrl: 'http://127.0.0.1:17034/v1?tenant=one',
    });
    expect(await findReusablePiTokenProvider({
      draft: { ...draft, baseUrl: 'http://127.0.0.1:17034/v1/?tenant=one' },
      providers: [tenant],
    })).toMatchObject({ id: 'pi-tenant' });
    expect(await findReusablePiTokenProvider({
      draft: { ...draft, baseUrl: 'http://127.0.0.1:17034/v1?tenant=two' },
      providers: [tenant],
    })).toBeNull();
  });
});

describe('resolveTokenImportProfile', () => {
  it('prefers the live profile, then the sibling with the same id', () => {
    const live = { id: 'live' } as AdapterProfile;
    const listed = { id: 'profile-1' } as AdapterProfile;
    expect(resolveTokenImportProfile(live, 'profile-1', [listed])).toBe(live);
    expect(resolveTokenImportProfile(null, 'profile-1', [listed])).toBe(listed);
    expect(resolveTokenImportProfile(undefined, 'missing', [listed])).toBeNull();
  });
});
