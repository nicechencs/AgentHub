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
  isCreateRouteUrlValid,
  isOpenRouterUrl,
  parseCreateRouteModels,
  readCreateRouteCapabilities,
  listLocalRouteSurfacesFromConfig,
  applyLocalRouteToAgents,
  submitCreateRoute,
  submitImportRoute,
  endpointUrlFor,
  upstreamEndpointPathForTarget,
  upstreamEndpointPathForUrl,
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

  it('fills a friendly default name from the vendor label alone', () => {
    expect(defaultCreateRouteName('OpenRouter')).toBe('OpenRouter');
    expect(defaultCreateRouteName('  智谱  ')).toBe('智谱');
    expect(defaultCreateRouteName('  Open  Router  ')).toBe('Open Router');
  });

  it('treats empty or any vendor default name as auto and keeps a typed name', () => {
    const autos = createRouteAutoNames(['OpenRouter', '智谱', '自定义']);
    expect(autos).toEqual(['OpenRouter', '智谱', '自定义']);
    expect(isAutoCreateRouteName('', autos)).toBe(true);
    expect(isAutoCreateRouteName('智谱', autos)).toBe(true);
    expect(isAutoCreateRouteName('OpenRouter', autos)).toBe(true);
    expect(isAutoCreateRouteName('家里的备用', autos)).toBe(false);
    expect(nextCreateRouteName('', '智谱', autos)).toBe('智谱');
    expect(nextCreateRouteName('OpenRouter', '智谱', autos)).toBe('智谱');
    expect(nextCreateRouteName('智谱', 'OpenRouter', autos)).toBe('OpenRouter');
    expect(nextCreateRouteName('家里的备用', 'OpenRouter', autos)).toBe('家里的备用');
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

  it('lists Claude messages plus Codex and Grok responses from capabilities', () => {
    const draft = createRouteProviderDraft(input({
      endpoints: ['claude', 'codex', 'grok'],
    }));
    const surfaces = listLocalRouteSurfacesFromConfig(draft.configText, {
      targetAgentId: 'claude',
      ruleId: 'openai-api-to-claude-v1',
    });
    expect(surfaces.map((row) => [row.target, row.path])).toEqual([
      ['claude', '/v1/messages'],
      ['codex', '/v1/responses'],
      ['grok', '/v1/responses'],
    ]);
  });

  it('falls back to all three surfaces for OpenRouter when endpoints are missing', () => {
    const surfaces = listLocalRouteSurfacesFromConfig(
      JSON.stringify({ vendor: 'openrouter', baseURL: 'https://openrouter.ai/api/v1' }),
      { targetAgentId: 'claude', ruleId: 'openai-api-to-claude-v1' },
    );
    expect(surfaces.map((row) => row.target)).toEqual(['claude', 'codex', 'grok']);
  });

  it('applies one ticket to each selected agent via plan and bind', async () => {
    const planTicket = vi.fn(async (_ticket: string, agent: string) =>
      plan(agent as AdapterApplyPlan['targetAgentId']),
    );
    const bindTicket = vi.fn(async (_ticket: string, agent: string) =>
      bindResult(agent as BindTicketResult['binding']['agentId']),
    );
    const applied = await applyLocalRouteToAgents(
      { sourceKind: 'provider', sourceId: 'or-1', agents: ['claude', 'codex', 'grok'] },
      { planTicket, bindTicket },
    );
    expect(applied).toEqual(['claude', 'codex', 'grok']);
    expect(planTicket).toHaveBeenCalledTimes(3);
    expect(bindTicket).toHaveBeenCalledTimes(3);
    expect(bindTicket.mock.calls.map((call) => call[1])).toEqual(['claude', 'codex', 'grok']);
  });

  it('upserts one provider and binds every checked client', async () => {
    const upsertProvider = vi.fn(async (provider: Provider) => provider);
    const planTicket = vi.fn(async (_ticket: string, agent: string) =>
      plan(agent as AdapterApplyPlan['targetAgentId']),
    );
    const bindTicket = vi.fn(async (_ticket: string, agent: string) =>
      bindResult(agent as BindTicketResult['binding']['agentId']),
    );

    const bound = await submitCreateRoute(input(), { upsertProvider, planTicket, bindTicket });

    expect(upsertProvider).toHaveBeenCalledOnce();
    expect(planTicket).toHaveBeenCalledTimes(3);
    expect(bindTicket).toHaveBeenCalledTimes(3);
    expect(bound).toEqual(['claude', 'codex', 'grok']);
    expect(bindTicket.mock.calls.map((call) => call[1])).toEqual(['claude', 'codex', 'grok']);
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

  it('omits a profile source even when route is native_endpoint', () => {
    const keys = alreadyRoutedSourceKeys([
      { id: 'p2', sourceKind: 'provider', sourceId: 'prov-1', route: 'native_endpoint' },
    ]);
    expect([...keys]).toEqual(['provider:prov-1']);
    expect(importableConnectionEntries(entries, keys).map((row) => row.id)).toEqual(['acc-1', 'acc-2']);
  });

  it('omits generatedProviderId as a provider login', () => {
    const keys = alreadyRoutedSourceKeys([
      {
        id: 'p-gen',
        sourceKind: 'account',
        sourceId: 'acc-1',
        route: 'local_bridge',
        generatedProviderId: 'gen-local-1',
      },
    ]);
    expect(keys.has('account:acc-1')).toBe(true);
    expect(keys.has('provider:gen-local-1')).toBe(true);
    const mixed = [
      ...entries,
      { source: 'provider' as const, id: 'gen-local-1' },
    ];
    expect(importableConnectionEntries(mixed, keys).map((row) => row.id)).toEqual(['prov-1', 'acc-2']);
  });

  it('omits a 本机路由 loopback login even without a matching profile key', () => {
    const leftover = {
      source: 'provider' as const,
      id: 'loop-1',
      title: '本机路由',
      endpointHost: '127.0.0.1',
      provider: {
        id: 'loop-1',
        name: '本机路由',
        preset: 'openai-compat',
        configText: '{"base_url":"http://127.0.0.1:43111/v1"}',
        configFormat: 'json' as const,
      },
    };
    expect(importableConnectionEntries([leftover, entries[2]!], new Set()).map((row) => row.id)).toEqual(['acc-2']);
  });

  it('keeps a never-routed oauth account importable', () => {
    const oauth = { source: 'account' as const, id: 'acc-oauth', title: 'Claude 登录' };
    const keys = alreadyRoutedSourceKeys([
      { id: 'p-or', sourceKind: 'provider', sourceId: 'or-1', route: 'native_endpoint' },
    ]);
    expect(importableConnectionEntries([oauth, { source: 'provider' as const, id: 'or-1', title: 'OpenRouter 备选' }], keys).map((row) => row.id)).toEqual(['acc-oauth']);
  });

  it('create-route OpenRouter 备选 id matches the bound profile sourceId', () => {
    const draft = createRouteProviderDraft(input({ name: 'OpenRouter 备选' }));
    const keys = alreadyRoutedSourceKeys([
      {
        id: 'profile-or',
        sourceKind: 'provider',
        sourceId: draft.id,
        route: 'native_endpoint',
        generatedProviderId: 'gen-or',
      },
    ]);
    expect(keys.has(`provider:${draft.id}`)).toBe(true);
    expect(importableConnectionEntries(
      [{ source: 'provider' as const, id: draft.id, title: 'OpenRouter 备选' }],
      keys,
    )).toEqual([]);
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

  const liveSourceId = 'openai-compat-0e08e310-97ba-4575-a50b-3e3db6eec38c';
  const liveTrunc = 'openai-compat-0e08e310-97ba-4575-a50b-3e';
  const liveBackup = 'openai-compat-openrouter-backup';

  it('omits same-titled leftover OpenRouter 备选 when UUID source is already routed', () => {
    const profiles = [
      {
        id: 'p-claude',
        name: 'OpenAI → Claude Code Bridge (openai-compat-0e08e310-97ba-4575-a50b-3e)',
        sourceKind: 'provider' as const,
        sourceId: liveSourceId,
        route: 'local_bridge',
        generatedProviderId: `claude-openai-adapter-bridge-${liveTrunc}-da544ba97deeffbb`,
      },
    ];
    const keys = alreadyRoutedSourceKeys(profiles);
    expect(keys.has(`provider:${liveSourceId}`)).toBe(true);
    expect(keys.has(`provider:${liveBackup}`)).toBe(false);
    const leftover = {
      source: 'provider' as const,
      id: liveBackup,
      title: 'OpenRouter 备选',
      provider: { id: liveBackup, name: 'OpenRouter 备选', preset: 'openrouter', configText: '{}', configFormat: 'json' as const },
    };
    const routedLogin = {
      source: 'provider' as const,
      id: liveSourceId,
      title: 'OpenRouter 备选',
      provider: { id: liveSourceId, name: 'OpenRouter 备选', preset: 'openrouter', configText: '{}', configFormat: 'json' as const },
    };
    const gmail = { source: 'account' as const, id: 'acc-gmail', title: 'cunsen.chen@gmail.com' };
    const qq = { source: 'account' as const, id: 'acc-qq', title: '41375197@qq.com' };
    expect(importableConnectionEntries(
      [leftover, routedLogin, gmail, qq],
      keys,
      profiles,
    ).map((row) => row.id)).toEqual(['acc-gmail', 'acc-qq']);
  });

  it('omits a provider when profile.name equals entry.title', () => {
    const profiles = [
      { id: 'p', name: 'OpenRouter 备选', sourceKind: 'provider' as const, sourceId: liveSourceId, route: 'native_endpoint' },
    ];
    const leftover = { source: 'provider' as const, id: liveBackup, title: 'OpenRouter 备选' };
    expect(importableConnectionEntries([leftover], alreadyRoutedSourceKeys(profiles), profiles)).toEqual([]);
  });

  it('omits when profile.sourceId is a prefix, suffix, or ticket wrapper of the entry id', () => {
    const prefixProfile = [
      { id: 'p', name: 'bridge', sourceKind: 'provider' as const, sourceId: liveTrunc, route: 'local_bridge' },
    ];
    const full = { source: 'provider' as const, id: liveSourceId, title: 'OpenRouter 备选' };
    expect(importableConnectionEntries([full], alreadyRoutedSourceKeys(prefixProfile), prefixProfile)).toEqual([]);

    const wrapped = [
      { id: 'p2', name: 'bridge', sourceKind: 'provider' as const, sourceId: `provider:${liveSourceId}`, route: 'local_bridge' },
    ];
    expect(importableConnectionEntries(
      [{ source: 'provider' as const, id: liveSourceId, title: 'x' }],
      alreadyRoutedSourceKeys(wrapped),
      wrapped,
    )).toEqual([]);
  });

  it('omits when entry.provider.id appears in profile sourceId or generatedProviderId', () => {
    const profiles = [
      {
        id: 'p',
        name: 'bridge',
        sourceKind: 'account' as const,
        sourceId: 'acc-1',
        route: 'local_bridge',
        generatedProviderId: `claude-openai-adapter-bridge-${liveSourceId}-tail`,
      },
    ];
    const entry = {
      source: 'provider' as const,
      id: 'unrelated-row',
      title: 'other',
      provider: { id: liveSourceId, name: 'OpenRouter 备选', preset: 'openrouter', configText: '{}', configFormat: 'json' as const },
    };
    expect(importableConnectionEntries([entry], alreadyRoutedSourceKeys(profiles), profiles)).toEqual([]);
  });

  it('does not hide never-routed gmail/qq oauth without a profile', () => {
    const profiles = [
      { id: 'p', name: 'OpenRouter 备选', sourceKind: 'provider' as const, sourceId: liveSourceId, route: 'native_endpoint' },
    ];
    const gmail = { source: 'account' as const, id: 'acc-gmail', title: 'cunsen.chen@gmail.com' };
    const qq = { source: 'account' as const, id: 'acc-qq', title: '41375197@qq.com' };
    expect(importableConnectionEntries([gmail, qq], alreadyRoutedSourceKeys(profiles), profiles).map((row) => row.id))
      .toEqual(['acc-gmail', 'acc-qq']);
  });

  it('infers upstream API paths from provider URLs', () => {
    expect(upstreamEndpointPathForUrl('https://openrouter.ai/api/v1')).toBe('/v1/chat/completions');
    expect(upstreamEndpointPathForUrl('https://open.bigmodel.cn/api/anthropic')).toBe('/v1/messages');
    expect(upstreamEndpointPathForUrl('https://api.x.ai/v1')).toBe('/v1/chat/completions');
    expect(upstreamEndpointPathForTarget('glm', 'claude', 'https://open.bigmodel.cn/api/coding/paas/v4'))
      .toBe('/v1/messages');
    expect(upstreamEndpointPathForTarget('glm', 'codex', 'https://open.bigmodel.cn/api/coding/paas/v4'))
      .toBe('/v1/chat/completions');
  });

  it('honours per-client upstream URL overrides when building endpoints', () => {
    const rows = buildCreateRouteEndpoints(
      'custom',
      'https://relay.example.com/v1',
      ['claude', 'codex'],
      {
        claude: 'https://relay.example.com/anthropic',
      },
    );
    expect(rows).toEqual([
      { target: 'claude', enabled: true, url: 'https://relay.example.com/anthropic' },
      { target: 'codex', enabled: true, url: 'https://relay.example.com/v1' },
      { target: 'grok', enabled: false, url: 'https://relay.example.com/v1' },
    ]);
    expect(endpointUrlFor('custom', 'claude', 'https://relay.example.com/v1', {
      claude: 'https://relay.example.com/anthropic',
    })).toBe('https://relay.example.com/anthropic');
  });

  it('persists per-client upstream URLs in the provider draft', () => {
    const draft = createRouteProviderDraft(input({
      vendor: 'custom',
      endpointUrls: { claude: 'https://relay.example.com/anthropic' },
    }));
    const parsed = JSON.parse(draft.configText);
    expect(parsed.endpoints).toEqual([
      { target: 'claude', enabled: true, url: 'https://relay.example.com/anthropic' },
      { target: 'codex', enabled: true, url: DEFAULT_CREATE_ROUTE_URL },
      { target: 'grok', enabled: true, url: DEFAULT_CREATE_ROUTE_URL },
    ]);
  });
});
