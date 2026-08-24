import { describe, expect, it } from 'vitest';
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';
import type { ConnectionEntry } from '@/lib/connection-entry';
import {
  appliedTargetsFromProfiles,
  bridgeHostPortLabel,
  bridgeNodeStatusLine,
  buildRouteDetailEdges,
  buildRouteDetailSourceView,
  defaultApplySelection,
  detectUpstreamChannelFromCredential,
  detectUpstreamChannelFromUrl,
  hopForTestable,
  routeCopyPortPendingLabel,
  routeDetailApplyConfirmLabel,
  routeDetailTargetLabel,
  routeEdgeSupportLabel,
  routeHopLabel,
  routeModelsSummary,
  routeSourceDeletedHint,
  selectableProductTargets,
  upstreamChannelLabel,
} from './adapter-route-detail-model';

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

  it('treats OpenRouter URLs as OpenAI Chat', () => {
    expect(detectUpstreamChannelFromUrl('https://openrouter.ai/api/v1')).toBe('openai_chat');
  });
});

describe('detectUpstreamChannelFromCredential', () => {
  it('maps oauth agents to fixed channels and ignores api mode for codex', () => {
    expect(detectUpstreamChannelFromCredential({ mode: 'oauth', sourceAgentId: 'codex' }))
      .toBe('codex_responses');
    expect(detectUpstreamChannelFromCredential({ mode: 'oauth', sourceAgentId: 'claude' }))
      .toBe('anthropic_messages');
    expect(detectUpstreamChannelFromCredential({ mode: 'api', sourceAgentId: 'codex' }))
      .toBe('unknown');
  });

  it('maps grok oauth and kimi/dsh api to expected channels', () => {
    expect(detectUpstreamChannelFromCredential({ mode: 'oauth', sourceAgentId: 'grok' }))
      .toBe('grok_responses');
    expect(detectUpstreamChannelFromCredential({ mode: 'oauth', sourceAgentId: 'xai' }))
      .toBe('grok_responses');
    expect(detectUpstreamChannelFromCredential({ mode: 'api', sourceAgentId: 'kimi' }))
      .toBe('openai_chat');
    expect(detectUpstreamChannelFromCredential({ mode: 'api', sourceAgentId: 'dsh' }))
      .toBe('openai_chat');
    expect(detectUpstreamChannelFromCredential({ mode: 'oauth', sourceAgentId: 'unknown-vendor' }))
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
  it('marks applied from siblings and ready for other product targets', () => {
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
          { target: 'grok', enabled: true, url: 'https://openrouter.ai/api/v1' },
        ],
      })],
      siblingProfiles: [],
      hiddenTargetIds: new Set(['claude']),
    });
    const byTarget = Object.fromEntries(edges.map((edge) => [edge.target, edge]));
    expect(byTarget.claude?.support).toBe('hidden');
    expect(byTarget.codex?.support).toBe('ready');
  });

  it('emits no_upstream for declared-endpoint gaps', () => {
    const edges = buildRouteDetailEdges({
      profile: profile({ targetAgentId: 'claude' }),
      entries: [entry({
        endpoints: [
          { target: 'claude', enabled: true, url: 'https://openrouter.ai/api/v1' },
        ],
      })],
      siblingProfiles: [],
    });
    const byTarget = Object.fromEntries(edges.map((edge) => [edge.target, edge]));
    expect(byTarget.claude?.support).toBe('ready');
    expect(byTarget.codex?.support).toBe('no_upstream');
    expect(byTarget.grok?.support).toBe('no_upstream');
  });

  it('does not emit no_upstream when endpoints field is empty', () => {
    const edges = buildRouteDetailEdges({
      profile: profile({ targetAgentId: 'codex', ruleId: 'bridge' }),
      entries: [entry({ baseURL: 'https://api.openai.com/v1' })],
      siblingProfiles: [],
    });
    expect(edges.some((edge) => edge.support === 'no_upstream')).toBe(false);
    expect(edges.every((edge) => edge.support === 'ready' || edge.support === 'applied')).toBe(true);
  });

  it('shows kimi only when applied sibling exists as runtime_only', () => {
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
    const claude = edges.find((edge) => edge.target === 'claude');
    expect(claude?.hop).toBe('passthrough');
    expect(routeHopLabel(claude!.hop, claude!.upstreamChannel)).toBe('直通上游');
  });
});

describe('hopForTestable', () => {
  it('uses passthrough / convert / forward from endpoint × channel', () => {
    expect(hopForTestable('messages', 'anthropic_messages')).toBe('passthrough');
    expect(hopForTestable('messages', 'openai_chat')).toBe('convert');
    expect(hopForTestable('messages', 'unknown')).toBe('forward');
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

  it('falls back to oauth credential channel when no usable URL exists', () => {
    const view = buildRouteDetailSourceView({
      profile: profile({ sourceKind: 'account', sourceId: 'acct-1', mode: 'oauth' }),
      entries: [entry({}, {
        source: 'account',
        id: 'acct-1',
        key: 'account:acct-1',
        agentId: 'codex',
      })],
    });
    expect(view.baseUrl).toBe('');
    expect(view.channel).toBe('codex_responses');
    expect(view.missing).toBe(false);
  });

  it('marks missing source when entry is gone', () => {
    const view = buildRouteDetailSourceView({
      profile: profile(),
      entries: [],
    });
    expect(view.missing).toBe(true);
    expect(view.baseUrl).toBe('');
    expect(view.upstreamUrls).toEqual([]);
    expect(view.channel).toBe('unknown');
  });
});

describe('bridge helpers', () => {
  it('omits {port} literal when pending', () => {
    expect(bridgeHostPortLabel({ host: '127.0.0.1', port: null })).toBe('127.0.0.1 · 端口分配中');
    expect(bridgeHostPortLabel({ host: '127.0.0.1', port: null })).not.toContain('{port}');
    expect(bridgeHostPortLabel({ host: '127.0.0.1', port: 43121 })).toBe('127.0.0.1:43121');
    expect(bridgeHostPortLabel({ host: '0.0.0.0', port: null })).toBe('0.0.0.0 · 端口分配中');
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

  it('joins runtime and upstream unless status is unavailable', () => {
    expect(bridgeNodeStatusLine({
      runtimeLabel: '运行中',
      upstreamLabel: '已连接',
      bridgeState: 'running',
    }).line).toBe('运行中 · 已连接');
    expect(bridgeNodeStatusLine({
      runtimeLabel: '运行中',
      upstreamLabel: '已连接',
      statusUnavailable: true,
    }).line).toBe('运行中');
  });
});

describe('label helpers', () => {
  it('maps edge support states to user-facing copy', () => {
    expect(routeEdgeSupportLabel('source_missing', 'Codex')).toBe('来源登录已删除');
    expect(routeEdgeSupportLabel('hidden', 'Codex')).toBe('该客户端已在设置中隐藏');
    expect(routeEdgeSupportLabel('no_upstream', 'Codex')).toBe('来源未配置此客户端的上游端点');
    expect(routeEdgeSupportLabel('applied', 'Codex')).toBe('已写入 Codex 配置');
    expect(routeEdgeSupportLabel('ready', 'Codex')).toBe('可一键接入');
    expect(routeEdgeSupportLabel('runtime_only', 'Kimi')).toBe('由后端路由支持，暂不提供界面配置');
  });

  it('maps upstream channels and hop labels', () => {
    expect(upstreamChannelLabel('openai_chat')).toBe('上游 Chat 接口');
    expect(upstreamChannelLabel('anthropic_messages')).toBe('上游 Messages');
    expect(upstreamChannelLabel('codex_responses')).toBe('上游 Codex Responses');
    expect(upstreamChannelLabel('grok_responses')).toBe('上游 Grok Responses');
    expect(upstreamChannelLabel('unknown')).toBe('上游');
    expect(routeHopLabel('convert', 'openai_chat')).toBe('转换 → 上游 Chat 接口');
  });

  it('summarizes models and static route copy', () => {
    expect(routeModelsSummary([])).toBe('跟随客户端请求的模型');
    expect(routeModelsSummary(['gpt-4o', 'claude-3'])).toBe('仅放行：gpt-4o, claude-3（其余模型将被拒绝）');
    expect(routeSourceDeletedHint()).toBe('来源登录已删除，路由仅可查看或解除绑定');
    expect(routeDetailApplyConfirmLabel()).toBe('将勾选项写入客户端配置');
    expect(routeCopyPortPendingLabel()).toBe('端口分配后可复制');
  });

  it('labels product and runtime targets', () => {
    expect(routeDetailTargetLabel('claude')).toBe('Claude');
    expect(routeDetailTargetLabel('codex')).toBe('Codex');
    expect(routeDetailTargetLabel('grok')).toBe('Grok');
    expect(routeDetailTargetLabel('kimi')).toBe('Kimi');
    expect(routeDetailTargetLabel('dsh')).toBe('DSH');
  });
});

describe('apply selection helpers', () => {
  it('defaults to applied selectable edges only', () => {
    const edges = buildRouteDetailEdges({
      profile: profile({ targetAgentId: 'claude', ruleId: 'openai-api-to-claude-v1' }),
      entries: [entry({
        endpoints: [
          { target: 'claude', enabled: true, url: 'https://openrouter.ai/api/v1' },
          { target: 'codex', enabled: true, url: 'https://openrouter.ai/api/v1' },
          { target: 'grok', enabled: true, url: 'https://openrouter.ai/api/v1' },
        ],
      })],
      siblingProfiles: [
        profile({ targetAgentId: 'claude', generatedProviderId: 'g-claude' }),
        profile({ id: 'g2', targetAgentId: 'grok', generatedProviderId: 'g-grok' }),
      ],
    });
    expect(defaultApplySelection(edges)).toEqual(['claude', 'grok']);
    expect(selectableProductTargets(['grok', 'claude'])).toEqual(['claude', 'grok']);
    expect(selectableProductTargets(['kimi'])).toEqual([]);
  });
});
