import { describe, expect, it } from 'vitest';
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';
import type { ConnectionEntry } from '@/lib/connection-entry';
import {
  appliedTargetsFromProfiles,
  bridgeHostPortLabel,
  bridgeNodeStatusLine,
  buildRouteDetailEdges,
  buildRouteDetailSourceView,
  detectUpstreamChannelFromCredential,
  detectUpstreamChannelFromUrl,
  hopForTestable,
  routeHopLabel,
} from './adapter-route-detail-model';

// Re-export hop via local helper by importing internals through edges.
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
    localPort: 43121,
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

describe('detectUpstreamChannelFromUrl', () => {
  it('detects Anthropic Messages from /anthropic or api.anthropic.com', () => {
    expect(detectUpstreamChannelFromUrl('https://api.deepseek.com/anthropic')).toBe('anthropic_messages');
    expect(detectUpstreamChannelFromUrl('https://api.anthropic.com')).toBe('anthropic_messages');
  });

  it('treats OpenRouter / openai-compat URLs as OpenAI Chat', () => {
    expect(detectUpstreamChannelFromUrl('https://openrouter.ai/api/v1')).toBe('openai_chat');
  });
});

describe('detectUpstreamChannelFromCredential', () => {
  it('maps oauth agents to fixed channels', () => {
    expect(detectUpstreamChannelFromCredential({ mode: 'oauth', sourceAgentId: 'codex' }))
      .toBe('codex_responses');
    expect(detectUpstreamChannelFromCredential({ mode: 'oauth', sourceAgentId: 'claude' }))
      .toBe('anthropic_messages');
    expect(detectUpstreamChannelFromCredential({ mode: 'api', sourceAgentId: 'codex' }))
      .toBe('unknown');
  });
});

describe('appliedTargetsFromProfiles', () => {
  it('collects sibling targets with generatedProviderId on the same source', () => {
    const applied = appliedTargetsFromProfiles(
      [
        profile({ targetAgentId: 'claude', generatedProviderId: 'g-claude' }),
        profile({ id: 'b2', targetAgentId: 'codex', generatedProviderId: 'g-codex' }),
        profile({ id: 'b3', targetAgentId: 'grok', generatedProviderId: null }),
        profile({
          id: 'other',
          sourceId: 'other-src',
          targetAgentId: 'grok',
          generatedProviderId: 'g-grok',
        }),
        profile({ id: 'k1', targetAgentId: 'kimi', generatedProviderId: 'g-kimi' }),
      ],
      { sourceKind: 'provider', sourceId: 'prov-1' },
    );
    expect([...applied].sort()).toEqual(['claude', 'codex', 'kimi']);
  });
});

describe('buildRouteDetailEdges', () => {
  it('marks applied from siblings and ready for unchecked product targets', () => {
    const edges = buildRouteDetailEdges({
      profile: profile({ targetAgentId: 'claude', ruleId: 'openai-api-to-claude-v1' }),
      entries: [entry({
        vendor: 'openrouter',
        baseURL: 'https://openrouter.ai/api/v1',
        endpoints: [
          { target: 'claude', enabled: true, url: 'https://openrouter.ai/api/v1' },
          { target: 'codex', enabled: true, url: 'https://openrouter.ai/api/v1' },
          { target: 'grok', enabled: true, url: 'https://openrouter.ai/api/v1' },
        ],
      })],
      siblingProfiles: [
        profile({ targetAgentId: 'claude', generatedProviderId: 'g-claude' }),
      ],
    });
    const byTarget = Object.fromEntries(edges.map((edge) => [edge.target, edge]));
    expect(byTarget.claude?.support).toBe('applied');
    expect(byTarget.codex?.support).toBe('ready');
    expect(byTarget.grok?.support).toBe('ready');
    expect(byTarget.claude?.hop).toBe('convert');
    expect(byTarget.claude?.selectable).toBe(true);
  });

  it('applies hidden per edge target', () => {
    const edges = buildRouteDetailEdges({
      profile: profile(),
      entries: [entry({
        vendor: 'openrouter',
        endpoints: [
          { target: 'claude', enabled: true, url: 'https://openrouter.ai/api/v1' },
          { target: 'codex', enabled: true, url: 'https://openrouter.ai/api/v1' },
        ],
      })],
      siblingProfiles: [],
      hiddenTargetIds: new Set(['claude']),
    });
    const byTarget = Object.fromEntries(edges.map((edge) => [edge.target, edge]));
    expect(byTarget.claude?.support).toBe('hidden');
    expect(byTarget.codex?.support).toBe('ready');
  });

  it('emits no_upstream only when endpoints decls exist and target is missing', () => {
    const withDecls = buildRouteDetailEdges({
      profile: profile({ targetAgentId: 'claude' }),
      entries: [entry({
        endpoints: [
          { target: 'claude', enabled: true, url: 'https://openrouter.ai/api/v1' },
        ],
      })],
      siblingProfiles: [],
    });
    // surfaces only include declared endpoints when present
    expect(withDecls.every((edge) => edge.support !== 'no_upstream')).toBe(true);

    const openrouterAll = buildRouteDetailEdges({
      profile: profile({ targetAgentId: 'claude' }),
      entries: [entry({
        vendor: 'openrouter',
        baseURL: 'https://openrouter.ai/api/v1',
        endpoints: [
          { target: 'claude', enabled: true, url: 'https://openrouter.ai/api/v1' },
        ],
      })],
      siblingProfiles: [],
    });
    // listLocalRouteSurfacesFromConfig uses endpoints when non-empty → only claude surface
    expect(openrouterAll.map((e) => e.target)).toEqual(['claude']);
  });

  it('does not emit no_upstream when endpoints field is empty', () => {
    const edges = buildRouteDetailEdges({
      profile: profile({ targetAgentId: 'codex', ruleId: 'bridge' }),
      entries: [entry({ baseURL: 'https://api.openai.com/v1' })],
      siblingProfiles: [],
    });
    expect(edges.some((edge) => edge.support === 'no_upstream')).toBe(false);
    expect(edges[0]?.support).toBe('ready');
  });

  it('shows kimi/dsh only when applied sibling exists as runtime_only', () => {
    const without = buildRouteDetailEdges({
      profile: profile(),
      entries: [entry({ vendor: 'openrouter', baseURL: 'https://openrouter.ai/api/v1' })],
      siblingProfiles: [],
    });
    expect(without.some((edge) => edge.target === 'kimi' || edge.target === 'dsh')).toBe(false);

    const withKimi = buildRouteDetailEdges({
      profile: profile(),
      entries: [entry({ vendor: 'openrouter', baseURL: 'https://openrouter.ai/api/v1' })],
      siblingProfiles: [
        profile({ id: 'k', targetAgentId: 'kimi', generatedProviderId: 'g-kimi' }),
      ],
    });
    const kimi = withKimi.find((edge) => edge.target === 'kimi');
    expect(kimi?.support).toBe('runtime_only');
    expect(kimi?.selectable).toBe(false);
    expect(kimi?.endpointId).toBe('chat_completions');
  });

  it('marks all edges source_missing when source login is gone', () => {
    const edges = buildRouteDetailEdges({
      profile: profile(),
      entries: [],
      siblingProfiles: [
        profile({ generatedProviderId: 'g-codex' }),
      ],
    });
    expect(edges.length).toBeGreaterThan(0);
    expect(edges.every((edge) => edge.support === 'source_missing')).toBe(true);
    expect(edges.every((edge) => edge.hop === 'forward')).toBe(true);
  });

  it('uses passthrough for messages×anthropic', () => {
    const edges = buildRouteDetailEdges({
      profile: profile({ targetAgentId: 'claude', ruleId: 'openai-api-to-claude-v1' }),
      entries: [entry({
        endpoints: [
          { target: 'claude', enabled: true, url: 'https://api.deepseek.com/anthropic' },
        ],
      })],
      siblingProfiles: [],
    });
    expect(edges[0]?.hop).toBe('passthrough');
    expect(routeHopLabel(edges[0]!.hop, edges[0]!.upstreamChannel)).toBe('直通上游');
  });
});

describe('buildRouteDetailSourceView', () => {
  it('prefers baseURL and lists upstream URLs for diagnostics', () => {
    const view = buildRouteDetailSourceView({
      profile: profile(),
      entries: [entry({
        baseURL: 'https://openrouter.ai/api/v1',
        endpoints: [
          { target: 'claude', enabled: true, url: 'https://open.bigmodel.cn/api/anthropic' },
        ],
      })],
    });
    expect(view.baseUrl).toBe('https://openrouter.ai/api/v1');
    expect(view.upstreamUrls).toContain('https://openrouter.ai/api/v1');
    expect(view.upstreamUrls).toContain('https://open.bigmodel.cn/api/anthropic');
    expect(view.channel).toBe('openai_chat');
    expect(view.missing).toBe(false);
  });
});

describe('bridge helpers', () => {
  it('omits {port} literal when pending', () => {
    expect(bridgeHostPortLabel({ host: '127.0.0.1', port: null })).toBe('127.0.0.1 · 端口分配中');
    expect(bridgeHostPortLabel({ host: '127.0.0.1', port: null })).not.toContain('{port}');
  });

  it('shows stopped hint only when stopped', () => {
    expect(bridgeNodeStatusLine({
      runtimeLabel: '已停止',
      upstreamLabel: '已停止',
      bridgeState: 'stopped',
    }).stoppedHint).toContain('已停止');
    expect(bridgeNodeStatusLine({
      runtimeLabel: '运行中',
      upstreamLabel: '已连接',
      bridgeState: 'running',
    }).stoppedHint).toBeNull();
  });
});

// silence unused import if tree-shaken oddly
void hopForTestable;
