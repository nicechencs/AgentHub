import { describe, expect, it } from 'vitest';
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';
import type { ConnectionEntry } from '@/lib/connection-entry';
import {
  appliedTargetsFromProfiles,
  bridgeHostPortLabel,
  bridgeNodeStatusLine,
  buildRouteDetailSourceView,
  currentProviderIdsFromEntries,
  detectUpstreamChannelFromCredential,
  detectUpstreamChannelFromUrl,
  hopForTestable,
  routeCopyPortPendingLabel,
  routeDetailTargetLabel,
  routeHopLabel,
  routeModelsSummary,
  routeSourceDeletedHint,
  routeWriteTruthFrom,
  runningAdapterProfileIds,
  upstreamChannelLabel,
  writeStateForProfile,
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
  const source = { sourceKind: 'provider' as const, sourceId: 'prov-1' };

  it('does not treat a leftover stamp as 已写入 without current write truth', () => {
    const applied = appliedTargetsFromProfiles(
      [
        profile({ targetAgentId: 'claude', generatedProviderId: 'g-claude' }),
        profile({ id: 'b2', targetAgentId: 'codex', generatedProviderId: 'g-codex' }),
      ],
      source,
    );
    expect([...applied]).toEqual([]);
  });

  it('marks 已写入 only when the generated provider is current and the local gateway is running', () => {
    const applied = appliedTargetsFromProfiles(
      [
        profile({ id: 'claude-live', targetAgentId: 'claude', generatedProviderId: 'g-claude' }),
        profile({ id: 'codex-live', targetAgentId: 'codex', generatedProviderId: 'g-codex' }),
        profile({ id: 'b3', targetAgentId: 'grok', generatedProviderId: null }),
        profile({
          id: 'other',
          sourceId: 'other-src',
          targetAgentId: 'grok',
          generatedProviderId: 'g-grok',
        }),
        profile({ id: 'kimi-live', targetAgentId: 'kimi', generatedProviderId: 'g-kimi' }),
      ],
      source,
      {
        currentProviderByAgent: {
          claude: 'g-claude',
          codex: 'g-codex',
          kimi: 'g-kimi',
        },
        runningProfileIds: new Set(['claude-live', 'codex-live', 'kimi-live']),
      },
    );
    expect([...applied].sort()).toEqual(['claude', 'codex', 'kimi']);
  });

  it('clears 已写入 when the current provider moved to another local gateway', () => {
    const stale = profile({
      id: 'stopped-40661',
      targetAgentId: 'claude',
      generatedProviderId: 'g-old-40661',
      localPort: 40661,
    });
    const entries = [
      { source: 'provider', agentId: 'claude', isCurrent: true, id: 'g-live-44227' },
      { source: 'provider', agentId: 'claude', isCurrent: false, id: 'g-old-40661' },
    ];
    expect(currentProviderIdsFromEntries(entries)).toEqual({ claude: 'g-live-44227' });
    expect(runningAdapterProfileIds({
      'stopped-40661': { state: 'stopped' },
      'live-44227': { state: 'running' },
    })).toEqual(new Set(['live-44227']));
    const truth = routeWriteTruthFrom(entries, {
      'stopped-40661': { state: 'stopped' },
      'live-44227': { state: 'running' },
    });
    expect(writeStateForProfile(stale, source, truth)).toEqual({
      applied: false,
      writeNote: 'rewritten',
    });
    expect([...appliedTargetsFromProfiles([stale], source, truth)]).toEqual([]);
  });

  it('clears 已写入 when the current write still points here but the local gateway is stopped', () => {
    const stopped = profile({
      id: 'stopped-44227',
      targetAgentId: 'claude',
      generatedProviderId: 'g-live-44227',
    });
    expect(writeStateForProfile(stopped, source, {
      currentProviderByAgent: { claude: 'g-live-44227' },
      runningProfileIds: new Set(),
    })).toEqual({ applied: false, writeNote: 'stopped' });
  });
});

describe('hopForTestable', () => {
  it('uses passthrough / convert / forward from endpoint × channel', () => {
    expect(hopForTestable('messages', 'anthropic_messages')).toBe('passthrough');
    expect(hopForTestable('messages', 'openai_chat')).toBe('convert');
    expect(hopForTestable('messages', 'unknown')).toBe('forward');
    expect(hopForTestable('responses', 'codex_responses')).toBe('passthrough');
    expect(hopForTestable('responses', 'grok_responses')).toBe('passthrough');
    expect(hopForTestable('responses', 'openai_chat')).toBe('convert');
    expect(hopForTestable('chat_completions', 'openai_chat')).toBe('passthrough');
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
    expect(routeCopyPortPendingLabel()).toBe('端口分配后可复制');
  });

  it('labels the three client targets', () => {
    expect(routeDetailTargetLabel('claude')).toBe('Claude');
    expect(routeDetailTargetLabel('codex')).toBe('Codex');
    expect(routeDetailTargetLabel('grok')).toBe('Grok');
  });
});
