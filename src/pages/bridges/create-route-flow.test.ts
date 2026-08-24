import { describe, expect, it, vi } from 'vitest';
import type { AdapterApplyPlan } from '@/lib/backend/contracts/adapter';
import type { BindTicketResult } from '@/lib/backend/contracts/ticket';
import type { Provider } from '@/lib/types';
import {
  buildCreateRouteEndpoints,
  canSubmitCreateRoute,
  CREATE_ROUTE_VENDORS,
  createRouteProviderDraft,
  DEFAULT_CREATE_ROUTE_MODEL,
  DEFAULT_CREATE_ROUTE_URL,
  createRouteAutoNames,
  defaultCreateRouteEndpoints,
  defaultCreateRouteName,
  alreadyRoutedSourceKeys,
  importableConnectionEntries,
  importRouteRowTitle,
  importRouteTarget,
  isAutoCreateRouteName,
  nextCreateRouteName,
  isAlternateRouteRule,
  isCreateRouteUrlValid,
  isOpenRouterUrl,
  parseCreateRouteModels,
  readCreateRouteCapabilities,
  submitCreateRoute,
  submitImportRoute,
  vendorById,
} from './create-route-flow';

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

function input(overrides: Partial<Parameters<typeof createRouteProviderDraft>[0]> = {}) {
  return {
    name: 'OpenRouter',
    url: DEFAULT_CREATE_ROUTE_URL,
    key: 'test-key',
    vendor: 'openrouter' as const,
    endpoints: ['claude', 'codex', 'grok'] as const,
    models: DEFAULT_CREATE_ROUTE_MODEL,
    ...overrides,
  };
}

describe('create-route-flow', () => {
  it('classifies OpenRouter URL and never treats sk-or- as ahb_', () => {
    expect(isOpenRouterUrl('https://openrouter.ai/api/v1')).toBe(true);
    expect(isCreateRouteUrlValid('https://openrouter.ai/api/v1')).toBe(true);
    expect(isCreateRouteUrlValid('openrouter.ai/api/v1')).toBe(false);
    const draft = createRouteProviderDraft(input());
    expect(draft.preset).toBe('openrouter');
    expect(draft.official).toBe(false);
    expect(draft.configText).toContain('https://openrouter.ai/api/v1');
    expect(draft.configText).toContain('test-key');
    expect(draft.configText).not.toContain('ahb_');
    expect(draft.configText).not.toMatch(/sk-or-/);
    expect(JSON.parse(draft.configText).model).toBe('stealth/ox-alpha');
  });

  it('defaults OpenRouter to all three endpoints and stealth/ox-alpha', () => {
    expect(defaultCreateRouteEndpoints('openrouter')).toEqual(['claude', 'codex', 'grok']);
    expect(vendorById('openrouter').models).toEqual([DEFAULT_CREATE_ROUTE_MODEL]);
    expect(CREATE_ROUTE_VENDORS.map((vendor) => vendor.id)).toEqual([
      'openrouter', 'openai', 'glm', 'kimi', 'deepseek', 'grok', 'claude', 'custom',
    ]);
  });

  it('pre-checks only the clients each vendor actually supports', () => {
    expect(defaultCreateRouteEndpoints('openai')).toEqual(['codex', 'grok']);
    expect(defaultCreateRouteEndpoints('claude')).toEqual(['claude']);
    expect(defaultCreateRouteEndpoints('kimi')).toEqual(['codex', 'grok']);
    expect(defaultCreateRouteEndpoints('grok')).toEqual(['codex', 'grok']);
    expect(defaultCreateRouteEndpoints('custom')).toEqual([]);
  });

  it('fills protocol-specific default URLs without inventing hosts', () => {
    expect(buildCreateRouteEndpoints('glm', vendorById('glm').url, ['claude', 'codex']).map((row) => [row.target, row.url])).toEqual([
      ['claude', 'https://open.bigmodel.cn/api/anthropic'],
      ['codex', 'https://open.bigmodel.cn/api/coding/paas/v4'],
      ['grok', 'https://open.bigmodel.cn/api/coding/paas/v4'],
    ]);
    expect(buildCreateRouteEndpoints('deepseek', vendorById('deepseek').url, ['claude', 'codex']).find((row) => row.target === 'claude')?.url)
      .toBe('https://api.deepseek.com/anthropic');
    expect(vendorById('openai').url).toBe('https://api.openai.com/v1');
    expect(vendorById('kimi').url).toBe('https://api.moonshot.cn/v1');
    expect(vendorById('grok').url).toBe('https://api.x.ai/v1');
    expect(vendorById('claude').url).toBe('https://api.anthropic.com');
  });

  it('fills a friendly default name from vendor + alternate without extra spaces', () => {
    expect(defaultCreateRouteName('OpenRouter', '备选')).toBe('OpenRouter 备选');
    expect(defaultCreateRouteName('  智谱  ', '备选')).toBe('智谱 备选');
  });

  it('treats empty or any vendor default name as auto and keeps a typed name', () => {
    const autos = createRouteAutoNames(['OpenRouter', '智谱', '自定义'], '备选');
    expect(autos).toEqual(['OpenRouter 备选', '智谱 备选', '自定义 备选']);
    expect(isAutoCreateRouteName('', autos)).toBe(true);
    expect(isAutoCreateRouteName('智谱 备选', autos)).toBe(true);
    expect(isAutoCreateRouteName('OpenRouter 备选', autos)).toBe(true);
    expect(isAutoCreateRouteName('家里的备用', autos)).toBe(false);
    expect(nextCreateRouteName('', '智谱 备选', autos)).toBe('智谱 备选');
    expect(nextCreateRouteName('OpenRouter 备选', '智谱 备选', autos)).toBe('智谱 备选');
    expect(nextCreateRouteName('智谱 备选', 'OpenRouter 备选', autos)).toBe('OpenRouter 备选');
    expect(nextCreateRouteName('家里的备用', 'OpenRouter 备选', autos)).toBe('家里的备用');
  });

  it('builds a distinct import row title from title, client, and endpoint', () => {
    const a = importRouteRowTitle(
      { title: '本机路由', subtitle: '已配置', agentId: 'claude', source: 'provider', endpointMode: 'official' },
      { agent: 'Claude', officialEndpoint: '官方端点', customEndpoint: '自定义端点' },
    );
    const b = importRouteRowTitle(
      { title: '本机路由', subtitle: '已配置', agentId: 'codex', source: 'provider', endpointMode: 'custom' },
      { agent: 'Codex', officialEndpoint: '官方端点', customEndpoint: '自定义端点' },
    );
    expect(a).toBe('本机路由 · Claude · 官方端点');
    expect(b).toBe('本机路由 · Codex · 自定义端点');
    expect(a).not.toBe(b);
  });

  it('requires name, key, http(s) URL, and at least one endpoint', () => {
    expect(canSubmitCreateRoute(input({ url: 'openrouter.ai/api/v1' }))).toBe(false);
    expect(canSubmitCreateRoute(input({ endpoints: [] }))).toBe(false);
    expect(canSubmitCreateRoute(input({ name: '' }))).toBe(false);
    expect(canSubmitCreateRoute(input())).toBe(true);
  });

  it('parses an optional model list; empty means no pin', () => {
    expect(parseCreateRouteModels('')).toEqual([]);
    expect(parseCreateRouteModels('stealth/ox-alpha, other/model')).toEqual([
      'stealth/ox-alpha',
      'other/model',
    ]);
    const empty = createRouteProviderDraft(input({ vendor: 'openai', url: 'https://api.openai.com/v1', models: '' }));
    expect(JSON.parse(empty.configText).listedModels).toEqual([]);
    expect(JSON.parse(empty.configText).model).toBeUndefined();
  });

  it('stores enabled endpoints and listed models on the one provider', () => {
    const draft = createRouteProviderDraft(input({
      vendor: 'glm',
      url: vendorById('glm').url,
      endpoints: ['claude', 'codex'],
      models: 'glm-4',
    }));
    const caps = readCreateRouteCapabilities(draft.configText);
    expect(caps.models).toEqual(['glm-4']);
    expect(caps.endpoints.map((row) => row.target)).toEqual(['claude', 'codex']);
    expect(caps.endpoints[0]?.url).toBe('https://open.bigmodel.cn/api/anthropic');
  });

  it('marks openai-compat local-bridge rules as alternate', () => {
    expect(isAlternateRouteRule('openai-api-to-claude-v1')).toBe(true);
    expect(isAlternateRouteRule('kimi-membership-to-codex-v1')).toBe(false);
  });

  it('upserts one provider and binds one route', async () => {
    const upsertProvider = vi.fn(async (provider: Provider) => provider);
    const planTicket = vi.fn(async (_ticket: string, agent: string) =>
      plan(agent as AdapterApplyPlan['targetAgentId']),
    );
    const bindTicket = vi.fn(async (_ticket: string, agent: string) =>
      bindResult(agent as BindTicketResult['binding']['agentId']),
    );

    const bound = await submitCreateRoute(input(), { upsertProvider, planTicket, bindTicket });

    expect(upsertProvider).toHaveBeenCalledOnce();
    expect(planTicket).toHaveBeenCalledOnce();
    expect(bindTicket).toHaveBeenCalledOnce();
    expect(bound).toEqual(['claude']);
    expect(planTicket.mock.calls[0]?.[1]).toBe('claude');
  });

  it('imports an existing login with one bind', async () => {
    const planTicket = vi.fn(async (_ticket: string, agent: string) =>
      plan(agent as AdapterApplyPlan['targetAgentId']),
    );
    const bindTicket = vi.fn(async (_ticket: string, agent: string) =>
      bindResult(agent as BindTicketResult['binding']['agentId']),
    );
    expect(importRouteTarget('kimi')).toBe('codex');
    const target = await submitImportRoute(
      { sourceKind: 'account', sourceId: 'acc-1', agentId: 'claude' },
      { planTicket, bindTicket },
    );
    expect(target).toBe('claude');
    expect(planTicket).toHaveBeenCalledOnce();
    expect(bindTicket).toHaveBeenCalledOnce();
    expect(planTicket.mock.calls[0]?.[0]).toBe('account:acc-1');
  });
});

describe('importable vs already routed logins', () => {
  const entries = [
    { source: 'account' as const, id: 'acc-1' },
    { source: 'provider' as const, id: 'prov-1' },
    { source: 'account' as const, id: 'acc-2' },
  ];

  it('omits a login that already has a local-bridge profile', () => {
    const keys = alreadyRoutedSourceKeys([
      { id: 'p1', sourceKind: 'account', sourceId: 'acc-1', route: 'local_bridge' },
    ]);
    expect([...keys]).toEqual(['account:acc-1']);
    expect(importableConnectionEntries(entries, keys).map((row) => row.id)).toEqual(['prov-1', 'acc-2']);
  });

  it('does not treat a native_endpoint profile as already routed unless wallet-bound', () => {
    const unbound = alreadyRoutedSourceKeys([
      { id: 'p2', sourceKind: 'provider', sourceId: 'prov-1', route: 'native_endpoint' },
    ]);
    expect(importableConnectionEntries(entries, unbound)).toEqual(entries);

    const bound = alreadyRoutedSourceKeys(
      [{ id: 'p2', sourceKind: 'provider', sourceId: 'prov-1', route: 'native_endpoint' }],
      new Set(['p2']),
    );
    expect(importableConnectionEntries(entries, bound).map((row) => row.id)).toEqual(['acc-1', 'acc-2']);
  });

  it('matches sourceKind+sourceId so account and provider ids do not collide', () => {
    const keys = alreadyRoutedSourceKeys([
      { id: 'p3', sourceKind: 'account', sourceId: 'same', route: 'local_bridge' },
    ]);
    const mixed = [
      { source: 'account' as const, id: 'same' },
      { source: 'provider' as const, id: 'same' },
    ];
    expect(importableConnectionEntries(mixed, keys)).toEqual([{ source: 'provider', id: 'same' }]);
  });

  it('skips empty sourceId', () => {
    const keys = alreadyRoutedSourceKeys([
      { id: 'dirty', sourceKind: 'account', sourceId: '   ', route: 'local_bridge' },
    ]);
    expect(keys.size).toBe(0);
  });
});
