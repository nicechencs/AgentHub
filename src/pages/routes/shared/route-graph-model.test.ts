import { describe, expect, it } from 'vitest';
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';
import type { ConnectionEntry } from '@/lib/connection-entry';
import type { TranslateFn } from '@/lib/i18n';
import {
  buildRouteGraph,
  groupRouteGraphRowsByUpstream,
  joinUpstreamUrl,
  routeGraphLinkLabel,
  routeGraphLinkStyle,
  routeGraphSharesUpstreamEndpoint,
  routeGraphSupportedAgents,
  upstreamPathForChannel,
  type RouteGraphRow,
} from './route-graph-model';

function profile(partial: Partial<AdapterProfile> = {}): AdapterProfile {
  return {
    id: 'bridge-1',
    name: 'OpenRouter',
    sourceKind: 'provider',
    sourceId: 'prov-1',
    targetAgentId: 'codex',
    route: 'local_bridge',
    mode: 'api',
    status: 'active',
    ruleId: 'openai-api-to-codex-v1',
    ruleVersion: '1',
    generatedProviderId: 'codex-bridge-1',
    localPort: 26275,
    autoStart: true,
    createdAt: '2026-08-12T00:00:00Z',
    updatedAt: '2026-08-12T00:00:00Z',
    ...partial,
  };
}

function entry(config: unknown, partial: Partial<ConnectionEntry> = {}): ConnectionEntry {
  return {
    key: 'provider:prov-1',
    source: 'provider',
    kind: 'apikey',
    id: 'prov-1',
    agentId: 'claude',
    title: 'OpenRouter',
    subtitle: '已配置',
    isCurrent: true,
    authStatus: 'valid',
    authHealth: 'configured',
    sortKey: '',
    provider: {
      id: 'prov-1',
      agentId: 'claude',
      name: 'OpenRouter',
      preset: 'openrouter',
      configText: JSON.stringify(config),
      configFormat: 'json',
      isCurrent: false,
      official: false,
    },
    ...partial,
  };
}

const OPENROUTER_URL = 'https://openrouter.ai/api/v1';

function openRouterEntry(): ConnectionEntry {
  return entry({
    vendor: 'openrouter',
    endpoints: [
      { target: 'claude', enabled: true, url: OPENROUTER_URL },
      { target: 'codex', enabled: true, url: OPENROUTER_URL },
      { target: 'grok', enabled: true, url: OPENROUTER_URL },
    ],
  });
}

function glmEntry(): ConnectionEntry {
  return entry({
    vendor: 'glm',
    baseURL: 'https://open.bigmodel.cn/api/coding/paas/v4',
    endpoints: [
      { target: 'claude', enabled: true, url: 'https://open.bigmodel.cn/api/anthropic' },
      { target: 'codex', enabled: true, url: 'https://open.bigmodel.cn/api/coding/paas/v4' },
      { target: 'grok', enabled: true, url: 'https://open.bigmodel.cn/api/coding/paas/v4' },
    ],
  });
}

function byAgent(rows: readonly RouteGraphRow[]): Record<string, RouteGraphRow | undefined> {
  return Object.fromEntries(rows.map((row) => [row.agent, row]));
}

describe('upstreamPathForChannel', () => {
  it('maps every channel to its upstream path', () => {
    expect(upstreamPathForChannel('anthropic_messages')).toBe('/v1/messages');
    expect(upstreamPathForChannel('codex_responses')).toBe('/v1/responses');
    expect(upstreamPathForChannel('grok_responses')).toBe('/v1/responses');
    expect(upstreamPathForChannel('openai_chat')).toBe('/v1/chat/completions');
    expect(upstreamPathForChannel('unknown')).toBe('');
  });
});

describe('routeGraphLinkStyle', () => {
  it('draws passthrough solid and everything else dashed', () => {
    expect(routeGraphLinkStyle('passthrough')).toBe('solid');
    expect(routeGraphLinkStyle('convert')).toBe('dashed');
    expect(routeGraphLinkStyle('forward')).toBe('dashed');
  });
});

describe('joinUpstreamUrl', () => {
  it('joins base and path', () => {
    expect(joinUpstreamUrl('https://api.deepseek.com/anthropic', '/v1/messages'))
      .toBe('https://api.deepseek.com/anthropic/v1/messages');
  });

  it('strips trailing slashes on the base', () => {
    expect(joinUpstreamUrl('https://api.anthropic.com//', '/v1/messages'))
      .toBe('https://api.anthropic.com/v1/messages');
  });

  it('returns empty when either side is blank', () => {
    expect(joinUpstreamUrl('', '/v1/messages')).toBe('');
    expect(joinUpstreamUrl('   ', '/v1/messages')).toBe('');
    expect(joinUpstreamUrl('https://openrouter.ai/api/v1', '')).toBe('');
    expect(joinUpstreamUrl('https://openrouter.ai/api/v1', '  ')).toBe('');
  });

  it('de-duplicates a single overlapping leading segment', () => {
    expect(joinUpstreamUrl('https://openrouter.ai/api/v1', '/v1/chat/completions'))
      .toBe('https://openrouter.ai/api/v1/chat/completions');
    expect(joinUpstreamUrl('https://api.x.ai/v1/', '/v1/responses'))
      .toBe('https://api.x.ai/v1/responses');
  });
});

describe('buildRouteGraph', () => {
  it('reads listed models and window from stored config, not a model catalog', () => {
    const withWindow = buildRouteGraph({
      profile: profile(),
      entries: [entry({
        vendor: 'openrouter',
        listedModels: ['stealth/ox-alpha'],
        contextWindowTokens: 1_048_576,
        endpoints: [
          { target: 'claude', enabled: true, url: OPENROUTER_URL },
          { target: 'codex', enabled: true, url: OPENROUTER_URL },
        ],
      })],
      siblingProfiles: [],
      port: 26275,
    });
    expect(withWindow.listedModels).toEqual(['stealth/ox-alpha']);
    expect(withWindow.contextWindowTokens).toBe(1_048_576);

    const omitted = buildRouteGraph({
      profile: profile(),
      entries: [openRouterEntry()],
      siblingProfiles: [],
      port: 26275,
    });
    expect(omitted.listedModels).toEqual([]);
    expect(omitted.contextWindowTokens).toBeNull();
  });

  it('maps an OpenRouter source to three converting rows', () => {
    const graph = buildRouteGraph({
      profile: profile(),
      entries: [openRouterEntry()],
      siblingProfiles: [],
      port: 26275,
    });
    expect(graph.local).toEqual({
      host: '127.0.0.1',
      port: 26275,
      origin: 'http://127.0.0.1:26275',
    });
    expect(graph.rows.map((row) => row.agent)).toEqual(['claude', 'codex', 'grok']);
    const rows = byAgent(graph.rows);
    expect(rows.claude?.localPath).toBe('/v1/messages');
    expect(rows.claude?.localEndpointId).toBe('messages');
    expect(rows.claude?.localUrl).toBe('http://127.0.0.1:26275/v1/messages');
    expect(rows.codex?.localPath).toBe('/v1/responses');
    expect(rows.codex?.localUrl).toBe('http://127.0.0.1:26275/v1/responses');
    expect(rows.grok?.localPath).toBe('/v1/responses');
    expect(rows.grok?.localUrl).toBe('http://127.0.0.1:26275/v1/responses');
    for (const row of graph.rows) {
      expect(row.upstreamChannel).toBe('openai_chat');
      expect(row.upstreamBaseUrl).toBe(OPENROUTER_URL);
      expect(row.upstreamPath).toBe('/v1/chat/completions');
      expect(row.upstreamUrl).toBe('https://openrouter.ai/api/v1/chat/completions');
      expect(row.hop).toBe('convert');
      expect(row.link).toBe('dashed');
      expect(row.enabled).toBe(true);
      expect(row.applied).toBe(false);
    }
  });

  it('keeps a GLM Anthropic override as passthrough while the rest convert', () => {
    const graph = buildRouteGraph({
      profile: profile({ targetAgentId: 'claude', ruleId: 'openai-api-to-claude-v1' }),
      entries: [glmEntry()],
      siblingProfiles: [],
      port: 26275,
    });
    const rows = byAgent(graph.rows);
    expect(rows.claude?.upstreamBaseUrl).toBe('https://open.bigmodel.cn/api/anthropic');
    expect(rows.claude?.upstreamChannel).toBe('anthropic_messages');
    expect(rows.claude?.upstreamPath).toBe('/v1/messages');
    expect(rows.claude?.upstreamUrl).toBe('https://open.bigmodel.cn/api/anthropic/v1/messages');
    expect(rows.claude?.hop).toBe('passthrough');
    expect(rows.claude?.link).toBe('solid');
    for (const agent of ['codex', 'grok'] as const) {
      expect(rows[agent]?.upstreamChannel).toBe('openai_chat');
      expect(rows[agent]?.hop).toBe('convert');
      expect(rows[agent]?.link).toBe('dashed');
    }
  });

  it('emits a disabled endpoint row with enabled false', () => {
    const graph = buildRouteGraph({
      profile: profile(),
      entries: [entry({
        vendor: 'openrouter',
        baseURL: OPENROUTER_URL,
        endpoints: [
          { target: 'claude', enabled: true, url: OPENROUTER_URL },
          { target: 'codex', enabled: false, url: OPENROUTER_URL },
          { target: 'grok', enabled: true, url: OPENROUTER_URL },
        ],
      })],
      siblingProfiles: [],
      port: 26275,
    });
    const rows = byAgent(graph.rows);
    expect(graph.rows.map((row) => row.agent)).toEqual(['claude', 'codex', 'grok']);
    expect(rows.claude?.enabled).toBe(true);
    expect(rows.codex?.enabled).toBe(false);
    expect(rows.grok?.enabled).toBe(true);
    expect(rows.codex?.upstreamBaseUrl).toBe(OPENROUTER_URL);
  });

  it('leaves local URLs null while the port is pending', () => {
    const graph = buildRouteGraph({
      profile: profile(),
      entries: [openRouterEntry()],
      siblingProfiles: [],
      port: null,
    });
    expect(graph.local.port).toBeNull();
    expect(graph.local.origin).toBe('');
    expect(graph.rows.length).toBeGreaterThan(0);
    expect(graph.rows.every((row) => row.localUrl === null)).toBe(true);
  });

  it('forwards with an unknown channel when the source login is deleted', () => {
    const graph = buildRouteGraph({
      profile: profile(),
      entries: [],
      siblingProfiles: [],
      port: 26275,
    });
    expect(graph.source.missing).toBe(true);
    expect(graph.rows.length).toBeGreaterThan(0);
    for (const row of graph.rows) {
      expect(row.upstreamBaseUrl).toBe('');
      expect(row.upstreamChannel).toBe('unknown');
      expect(row.upstreamPath).toBe('');
      expect(row.upstreamUrl).toBe('');
      expect(row.hop).toBe('forward');
      expect(row.link).toBe('dashed');
    }
  });

  it('flags applied only from the current running write, not a leftover stamp', () => {
    const graph = buildRouteGraph({
      profile: profile(),
      entries: [openRouterEntry()],
      siblingProfiles: [
        profile({ id: 'g1', targetAgentId: 'grok', generatedProviderId: 'g-grok' }),
        profile({ id: 'g2', targetAgentId: 'claude', generatedProviderId: null }),
        profile({
          id: 'g3',
          sourceId: 'other-src',
          targetAgentId: 'codex',
          generatedProviderId: 'g-codex',
        }),
      ],
      port: 26275,
      writeTruth: {
        currentProviderByAgent: { grok: 'g-grok' },
        runningProfileIds: new Set(['g1']),
      },
    });
    const rows = byAgent(graph.rows);
    expect(rows.grok?.applied).toBe(true);
    expect(rows.grok?.writeNote).toBeNull();
    expect(rows.claude?.applied).toBe(false);
    expect(rows.codex?.applied).toBe(false);
  });

  it('does not show 已写入 on a stopped port after the current write moved', () => {
    const graph = buildRouteGraph({
      profile: profile({ id: 'stale-40661', localPort: 40661 }),
      entries: [openRouterEntry()],
      siblingProfiles: [
        profile({
          id: 'stale-40661',
          targetAgentId: 'claude',
          generatedProviderId: 'g-old-40661',
          localPort: 40661,
        }),
      ],
      port: 40661,
      writeTruth: {
        currentProviderByAgent: { claude: 'g-live-44227' },
        runningProfileIds: new Set(['live-44227']),
      },
    });
    const rows = byAgent(graph.rows);
    expect(rows.claude?.applied).toBe(false);
    expect(rows.claude?.writeNote).toBe('rewritten');
    expect(rows.claude?.localUrl).toContain(':40661/');
  });

  it('does not take a listen port from another login targeting the same client', () => {
    const graph = buildRouteGraph({
      profile: profile(),
      entries: [openRouterEntry()],
      siblingProfiles: [
        profile({
          id: 'stale-codex',
          sourceId: 'other-src',
          targetAgentId: 'codex',
          generatedProviderId: 'g-stale',
          localPort: 43623,
        }),
        profile({
          id: 'or-codex',
          targetAgentId: 'codex',
          generatedProviderId: 'g-or-codex',
          localPort: 40661,
        }),
      ],
      port: 40661,
    });
    const rows = byAgent(graph.rows);
    expect(rows.codex?.localUrl).toContain(':40661/');
    expect(rows.codex?.localUrl).not.toContain('43623');
  });

  it('honours a custom host', () => {
    const graph = buildRouteGraph({
      profile: profile(),
      entries: [openRouterEntry()],
      siblingProfiles: [],
      host: '0.0.0.0',
      port: 26275,
    });
    expect(graph.local.origin).toBe('http://0.0.0.0:26275');
    expect(graph.rows[0]?.localUrl).toBe('http://0.0.0.0:26275/v1/messages');
  });
});

describe('routeGraphSupportedAgents', () => {
  it('drops disabled agents and de-duplicates', () => {
    const graph = buildRouteGraph({
      profile: profile(),
      entries: [entry({
        vendor: 'openrouter',
        baseURL: OPENROUTER_URL,
        endpoints: [
          { target: 'claude', enabled: true, url: OPENROUTER_URL },
          { target: 'codex', enabled: false, url: OPENROUTER_URL },
          { target: 'grok', enabled: true, url: OPENROUTER_URL },
        ],
      })],
      siblingProfiles: [],
      port: 26275,
    });
    expect(routeGraphSupportedAgents(graph.rows)).toEqual(['claude', 'grok']);
    expect(routeGraphSupportedAgents([...graph.rows, ...graph.rows])).toEqual(['claude', 'grok']);
    expect(routeGraphSupportedAgents([])).toEqual([]);
  });
});

describe('routeGraphSharesUpstreamEndpoint', () => {
  it('returns true when every row shares the same upstream endpoint', () => {
    const graph = buildRouteGraph({
      profile: profile(),
      entries: [openRouterEntry()],
      siblingProfiles: [],
      port: 26275,
    });
    expect(routeGraphSharesUpstreamEndpoint(graph.rows)).toBe(true);
    expect(groupRouteGraphRowsByUpstream(graph.rows)).toHaveLength(1);
  });

  it('splits groups when upstream paths differ', () => {
    const graph = buildRouteGraph({
      profile: profile({ targetAgentId: 'claude', ruleId: 'openai-api-to-claude-v1' }),
      entries: [glmEntry()],
      siblingProfiles: [],
      port: 26275,
    });
    expect(routeGraphSharesUpstreamEndpoint(graph.rows)).toBe(false);
    expect(groupRouteGraphRowsByUpstream(graph.rows).length).toBeGreaterThan(1);
  });
});

describe('routeGraphLinkLabel', () => {
  it('falls back to Chinese copy without a translator', () => {
    expect(routeGraphLinkLabel('passthrough')).toBe('直通');
    expect(routeGraphLinkLabel('convert')).toBe('转换');
    expect(routeGraphLinkLabel('forward')).toBe('转发');
  });

  it('uses the translator when provided', () => {
    const t: TranslateFn = (key) => `t(${key})`;
    expect(routeGraphLinkLabel('passthrough', t)).toBe('t(routes.graph.linkPassthrough)');
    expect(routeGraphLinkLabel('convert', t)).toBe('t(routes.graph.linkConvert)');
    expect(routeGraphLinkLabel('forward', t)).toBe('t(routes.graph.linkForward)');
  });
});
