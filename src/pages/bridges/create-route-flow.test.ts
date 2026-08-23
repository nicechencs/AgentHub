import { describe, expect, it, vi } from 'vitest';
import type { AdapterApplyPlan } from '@/lib/backend/contracts/adapter';
import type { BindTicketResult } from '@/lib/backend/contracts/ticket';
import type { Provider } from '@/lib/types';
import {
  canSubmitCreateRoute,
  CREATE_ROUTE_VENDORS,
  createRouteProviderDraft,
  DEFAULT_CREATE_ROUTE_MODEL,
  DEFAULT_CREATE_ROUTE_URL,
  defaultCreateRouteClients,
  groupCreateRouteClientsByUrl,
  isAlternateRouteRule,
  isCreateRouteUrlValid,
  isOpenRouterUrl,
  resolveCreateRouteTargets,
  submitCreateRoute,
  urlForVendor,
  vendorIdForUrl,
  type CreateRouteClient,
} from './create-route-flow';

function clients(
  rows: Array<Partial<CreateRouteClient> & Pick<CreateRouteClient, 'target'>>,
): CreateRouteClient[] {
  return rows.map((row) => ({
    enabled: row.enabled ?? true,
    url: row.url ?? DEFAULT_CREATE_ROUTE_URL,
    target: row.target,
  }));
}

function plan(targetAgentId: AdapterApplyPlan['targetAgentId']): AdapterApplyPlan {
  return {
    analysis: {
      route: 'local_bridge',
      support: 'experimental',
      reason: 'test',
      actions: [],
      limitations: [],
      evidence: [],
    },
    targetAgentId,
    canApply: true,
    serviceImpact: 'requires_local_bridge',
    changes: [],
  };
}

function bindResult(agentId: BindTicketResult['binding']['agentId']): BindTicketResult {
  return {
    binding: {
      ticketId: 'provider:test',
      agentId,
      route: 'bridge',
      active: true,
      profileId: 'profile-1',
      bridge: null,
    },
  };
}

describe('create-route-flow', () => {
  it('classifies OpenRouter URL and never treats sk-or- as ahb_', () => {
    expect(isOpenRouterUrl('https://openrouter.ai/api/v1')).toBe(true);
    expect(isCreateRouteUrlValid('https://openrouter.ai/api/v1')).toBe(true);
    expect(isCreateRouteUrlValid('openrouter.ai/api/v1')).toBe(false);
    const draft = createRouteProviderDraft({
      name: 'OpenRouter',
      key: 'test-key',
      clients: clients([{ target: 'claude' }]),
      model: 'stealth/ox-alpha',
    }, DEFAULT_CREATE_ROUTE_URL, 'claude');
    expect(draft.preset).toBe('openrouter');
    expect(draft.official).toBe(false);
    expect(draft.configText).toContain('https://openrouter.ai/api/v1');
    expect(draft.configText).toContain('test-key');
    expect(draft.configText).not.toContain('ahb_');
    expect(draft.configText).not.toMatch(/sk-or-/);
    expect(JSON.parse(draft.configText).model).toBe('stealth/ox-alpha');
  });

  it('defaults all three clients checked with the OpenRouter URL', () => {
    expect(defaultCreateRouteClients()).toEqual([
      { target: 'claude', enabled: true, url: DEFAULT_CREATE_ROUTE_URL },
      { target: 'codex', enabled: true, url: DEFAULT_CREATE_ROUTE_URL },
      { target: 'grok', enabled: true, url: DEFAULT_CREATE_ROUTE_URL },
    ]);
  });

  it('does not auto-bind unchecked clients', () => {
    expect(resolveCreateRouteTargets([])).toEqual([]);
    expect(resolveCreateRouteTargets(clients([
      { target: 'claude', enabled: false },
      { target: 'codex', enabled: true },
      { target: 'grok', enabled: false },
    ]))).toEqual(['codex']);
  });

  it('rejects a URL that is not http(s)', () => {
    expect(canSubmitCreateRoute({
      name: 'x',
      key: 'k',
      clients: clients([{ target: 'claude', url: 'openrouter.ai/api/v1' }]),
    })).toBe(false);
  });

  it('requires at least one checked client', () => {
    expect(canSubmitCreateRoute({
      name: 'OpenRouter',
      key: 'test-key',
      clients: defaultCreateRouteClients().map((row) => ({ ...row, enabled: false })),
    })).toBe(false);
  });

  it('defaults omitted model to stealth/ox-alpha', () => {
    expect(canSubmitCreateRoute({
      name: 'OpenRouter',
      key: 'test-key',
      model: '',
      clients: clients([{ target: 'claude' }]),
    })).toBe(true);
    const draft = createRouteProviderDraft({
      name: 'OpenRouter',
      key: 'test-key',
      clients: defaultCreateRouteClients(),
    }, DEFAULT_CREATE_ROUTE_URL, 'codex');
    expect(JSON.parse(draft.configText).model).toBe(DEFAULT_CREATE_ROUTE_MODEL);
    expect(DEFAULT_CREATE_ROUTE_MODEL).toBe('stealth/ox-alpha');
  });

  it('groups shared URLs into one provider and splits when URLs differ', () => {
    expect(groupCreateRouteClientsByUrl(defaultCreateRouteClients())).toEqual([
      { url: DEFAULT_CREATE_ROUTE_URL, targets: ['claude', 'codex', 'grok'] },
    ]);
    expect(groupCreateRouteClientsByUrl(clients([
      { target: 'claude', url: DEFAULT_CREATE_ROUTE_URL },
      { target: 'codex', url: 'https://api.openai.com/v1' },
      { target: 'grok', enabled: false, url: DEFAULT_CREATE_ROUTE_URL },
    ]))).toEqual([
      { url: DEFAULT_CREATE_ROUTE_URL, targets: ['claude'] },
      { url: 'https://api.openai.com/v1', targets: ['codex'] },
    ]);
  });

  it('maps known official OpenAI-compat hosts and leaves others custom', () => {
    expect(CREATE_ROUTE_VENDORS.map((vendor) => vendor.id)).toEqual([
      'openrouter',
      'openai',
      'xai',
      'deepseek',
      'custom',
    ]);
    expect(vendorIdForUrl(DEFAULT_CREATE_ROUTE_URL)).toBe('openrouter');
    expect(vendorIdForUrl('https://api.openai.com/v1/')).toBe('openai');
    expect(vendorIdForUrl('https://api.x.ai/v1')).toBe('xai');
    expect(vendorIdForUrl('https://api.deepseek.com')).toBe('deepseek');
    expect(vendorIdForUrl('https://relay.example.com/v1')).toBe('custom');
    expect(urlForVendor('openai', DEFAULT_CREATE_ROUTE_URL)).toBe('https://api.openai.com/v1');
    expect(urlForVendor('custom', 'https://relay.example.com/v1')).toBe('https://relay.example.com/v1');
  });

  it('marks openai-compat local-bridge rules as alternate', () => {
    expect(isAlternateRouteRule('openai-api-to-claude-v1')).toBe(true);
    expect(isAlternateRouteRule('openai-api-to-codex-v1')).toBe(true);
    expect(isAlternateRouteRule('openai-api-to-grok-bridge-v1')).toBe(true);
    expect(isAlternateRouteRule('kimi-membership-to-codex-v1')).toBe(false);
  });

  it('upserts one provider then plans and binds only checked clients', async () => {
    const upsertProvider = vi.fn(async (provider: Provider) => provider);
    const planTicket = vi.fn(async (_ticket: string, agent: string) =>
      plan(agent as AdapterApplyPlan['targetAgentId']),
    );
    const bindTicket = vi.fn(async (_ticket: string, agent: string) =>
      bindResult(agent as BindTicketResult['binding']['agentId']),
    );

    const bound = await submitCreateRoute(
      {
        name: 'OpenRouter',
        key: 'test-key',
        clients: defaultCreateRouteClients(),
      },
      { upsertProvider, planTicket, bindTicket },
    );

    expect(upsertProvider).toHaveBeenCalledOnce();
    const saved = upsertProvider.mock.calls[0]?.[0];
    expect(saved?.preset).toBe('openrouter');
    expect(saved?.official).toBe(false);
    expect(saved?.configText).toContain('test-key');
    expect(saved?.configText).not.toMatch(/sk-or-/);
    expect(JSON.parse(saved?.configText ?? '{}').model).toBe(DEFAULT_CREATE_ROUTE_MODEL);
    expect(planTicket.mock.calls.map((call) => call[1])).toEqual(['claude', 'codex', 'grok']);
    expect(bindTicket.mock.calls.map((call) => call[1])).toEqual(['claude', 'codex', 'grok']);
    expect(bound).toEqual(['claude', 'codex', 'grok']);
    for (let i = 0; i < 3; i += 1) {
      const planOrder = planTicket.mock.invocationCallOrder[i];
      const bindOrder = bindTicket.mock.invocationCallOrder[i];
      expect(planOrder).toBeDefined();
      expect(bindOrder).toBeDefined();
      expect(planOrder!).toBeLessThan(bindOrder!);
    }
  });

  it('creates separate providers when checked client URLs differ', async () => {
    const upsertProvider = vi.fn(async (provider: Provider) => provider);
    const planTicket = vi.fn(async (_ticket: string, agent: string) =>
      plan(agent as AdapterApplyPlan['targetAgentId']),
    );
    const bindTicket = vi.fn(async (_ticket: string, agent: string) =>
      bindResult(agent as BindTicketResult['binding']['agentId']),
    );

    const bound = await submitCreateRoute(
      {
        name: 'Backup',
        key: 'test-key',
        clients: clients([
          { target: 'claude', url: DEFAULT_CREATE_ROUTE_URL },
          { target: 'codex', url: 'https://api.openai.com/v1' },
          { target: 'grok', enabled: false },
        ]),
      },
      { upsertProvider, planTicket, bindTicket },
    );

    expect(upsertProvider).toHaveBeenCalledTimes(2);
    expect(JSON.parse(upsertProvider.mock.calls[0]?.[0].configText ?? '{}').baseURL)
      .toBe(DEFAULT_CREATE_ROUTE_URL);
    expect(JSON.parse(upsertProvider.mock.calls[1]?.[0].configText ?? '{}').baseURL)
      .toBe('https://api.openai.com/v1');
    expect(planTicket.mock.calls.map((call) => call[1])).toEqual(['claude', 'codex']);
    expect(bound).toEqual(['claude', 'codex']);
  });
});
