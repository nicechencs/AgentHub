import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';
import { AdapterRoutePipeline } from './AdapterRoutePipeline';
import { AdapterTargetGrid } from './AdapterTargetGrid';
import {
  adapterApplySummaryLine,
  adapterBridgeFleetSummary,
  adapterConfigStatusView,
  adapterProfileFlowLabel,
  adapterProfilePrimaryAction,
  adapterProfileRecoveryGuide,
  adapterRoutePipelineModel,
  adapterServiceStatusView,
  adapterTargetBadge,
  adapterTargetCacheKey,
  bridgesPageViewState,
  bridgeRuntimeStatusView,
  filterBoundLocalBridgeRuntimes,
  partitionLocalBridgeRuntimes,
  resolveAdapterProfileSource,
} from './adapter-view-model';

function bridgeProfile(partial: Partial<AdapterProfile> = {}): AdapterProfile {
  return {
    id: 'bridge-1',
    name: 'Kimi → Codex',
    sourceKind: 'provider',
    sourceId: 'kimi-1',
    targetAgentId: 'codex',
    route: 'local_bridge',
    mode: 'api',
    status: 'active',
    ruleId: 'kimi-membership-to-codex-v1',
    ruleVersion: '1',
    generatedProviderId: 'codex-bridge-1',
    localPort: 32123,
    autoStart: false,
    createdAt: '2026-08-12T00:00:00Z',
    updatedAt: '2026-08-12T00:00:00Z',
    ...partial,
  };
}

describe('local-bridge runtime partition', () => {
  const entries = [
    { source: 'provider' as const, id: 'kimi-1' },
    { source: 'account' as const, id: 'acc-1' },
  ];

  it('keeps local_bridge profiles whose source still exists in bound', () => {
    const partitioned = partitionLocalBridgeRuntimes(
      [bridgeProfile(), bridgeProfile({ id: 'direct', route: 'native_endpoint', sourceId: 'kimi-1' })],
      { entries },
    );
    expect(partitioned.bound.map((item) => item.id)).toEqual(['bridge-1']);
    expect(partitioned.orphan).toEqual([]);
    expect(filterBoundLocalBridgeRuntimes(
      [bridgeProfile(), bridgeProfile({ id: 'direct', route: 'native_endpoint', sourceId: 'kimi-1' })],
      { entries },
    ).map((item) => item.id)).toEqual(['bridge-1']);
  });

  it('keeps a missing-source bridge in bound when wallet binding.profileId hits', () => {
    const missing = bridgeProfile({ id: 'missing-bound', sourceId: 'deleted' });
    expect(partitionLocalBridgeRuntimes([missing], { entries })).toEqual({
      bound: [],
      orphan: [missing],
    });
    expect(partitionLocalBridgeRuntimes([missing], {
      entries,
      bindingProfileIds: new Set(['missing-bound']),
    }).bound.map((item) => item.id)).toEqual(['missing-bound']);
  });

  it('lists a missing-source bridge without a binding hit as orphan', () => {
    const orphan = bridgeProfile({ id: 'orphan', sourceId: 'deleted' });
    const partitioned = partitionLocalBridgeRuntimes([orphan], { entries });
    expect(partitioned.bound).toEqual([]);
    expect(partitioned.orphan.map((item) => item.id)).toEqual(['orphan']);
  });

  it('drops bridges with no source id', () => {
    expect(partitionLocalBridgeRuntimes(
      [bridgeProfile({ sourceId: '' })],
      { entries, bindingProfileIds: new Set(['bridge-1']) },
    )).toEqual({ bound: [], orphan: [] });
  });
});

describe('bridges page view state', () => {
  const emptyWallet = { settled: true, lastWalletBridgeCount: 0 };

  it('stays loading until profiles and wallet have both settled', () => {
    expect(bridgesPageViewState({
      profileState: 'ready',
      bound: [],
      orphan: [],
      wallet: { settled: false, lastWalletBridgeCount: 0 },
    })).toBe('loading');
    expect(bridgesPageViewState({
      profileState: 'loading',
      bound: [],
      orphan: [],
      wallet: emptyWallet,
    })).toBe('loading');
  });

  it('uses last-known wallet count so a later wallet failure cannot become healthy empty', () => {
    expect(bridgesPageViewState({
      profileState: 'ready',
      bound: [],
      orphan: [],
      wallet: { settled: true, lastWalletBridgeCount: 1 },
    })).toBe('wallet_without_runtime');
  });

  it('treats only-orphan as a list, not healthy empty', () => {
    expect(bridgesPageViewState({
      profileState: 'ready',
      bound: [],
      orphan: [bridgeProfile({ id: 'orphan', sourceId: 'deleted' })],
      wallet: emptyWallet,
    })).toBe('list');
  });

  it('shows healthy empty only after both sides settle with zero bridges', () => {
    expect(bridgesPageViewState({
      profileState: 'ready',
      bound: [],
      orphan: [],
      wallet: emptyWallet,
    })).toBe('healthy_empty');
    expect(bridgesPageViewState({
      profileState: 'error',
      bound: [],
      orphan: [],
      wallet: emptyWallet,
    })).toBe('list_error');
  });
});

describe('adapter target panorama view model', () => {
  it('maps analyze conclusions onto route badges without claiming write access', () => {
    expect(adapterTargetBadge({ route: 'native_endpoint', support: 'stable' }))
      .toEqual({ label: '直连', variant: 'success' });
    expect(adapterTargetBadge({ route: 'local_bridge', support: 'experimental' }))
      .toEqual({ label: '桥接 · 实验', variant: 'warning' });
    expect(adapterTargetBadge({ route: 'local_bridge', support: 'stable' }))
      .toEqual({ label: '本地桥接', variant: 'warning' });
    expect(adapterTargetBadge({ route: 'config_sync', support: 'stable' }))
      .toEqual({ label: '配置同步', variant: 'info' });
    // Unsupported stays a neutral chip, never a red fault.
    expect(adapterTargetBadge({ route: 'unsupported', support: 'unsupported' }))
      .toEqual({ label: '暂不支持', variant: 'default' });
  });

  it('keys the analysis cache by source kind, source id, and target', () => {
    const key = adapterTargetCacheKey({ sourceKind: 'provider', sourceId: 'kimi-1', targetAgentId: 'codex' });
    expect(key).toBe('provider:kimi-1:codex');
    expect(key).not.toBe(adapterTargetCacheKey({ sourceKind: 'account', sourceId: 'kimi-1', targetAgentId: 'codex' }));
  });

  it('renders unconfigurable targets disabled and failed cards with a retry hint', () => {
    const markup = renderToStaticMarkup(
      createElement(AdapterTargetGrid, {
        agentIds: ['claude', 'codex', 'kimi'],
        configurableIds: new Set(['claude', 'codex']),
        analyses: {
          claude: { kind: 'ready', analysis: { route: 'native_endpoint', support: 'stable', reason: '', actions: [], limitations: [], evidence: [] } },
          codex: { kind: 'error', error: new Error('boom') },
        },
        selectedAgentId: 'claude',
        onSelect: vi.fn(),
        onRetry: vi.fn(),
      }),
    );
    expect(markup).toContain('直连');
    expect(markup).toContain('分析失败 · 点击重试');
    expect(markup).toContain('未安装或不可配置');
    expect(markup).toContain('disabled');
    expect(markup).toContain('aria-selected="true"');
  });
});

describe('adapter two-layer profile status', () => {
  it('maps the durable configuration lifecycle and pulses only transient states', () => {
    expect(adapterConfigStatusView('active')).toEqual({ label: '配置已生效', tone: 'success' });
    expect(adapterConfigStatusView('applying')).toEqual({ label: '应用中', tone: 'info', pulse: true });
    expect(adapterConfigStatusView('needs_attention')).toEqual({ label: '需要处理', tone: 'warning' });
  });

  it('keeps runtime state separate and never renders one for direct routes', () => {
    expect(bridgeRuntimeStatusView({ route: 'native_endpoint' })).toBeNull();
    expect(adapterServiceStatusView({ route: 'config_sync' })).toBeNull();
    expect(bridgeRuntimeStatusView({ route: 'local_bridge', bridgeState: 'running' }))
      .toEqual({ label: '运行中', tone: 'success' });
    expect(bridgeRuntimeStatusView({ route: 'local_bridge', bridgeState: 'starting' }))
      .toEqual({ label: '启动中', tone: 'info', pulse: true });
    expect(bridgeRuntimeStatusView({ route: 'local_bridge', bridgeState: 'degraded' }))
      .toEqual({ label: '已降级', tone: 'warning' });
    expect(bridgeRuntimeStatusView({ route: 'local_bridge', bridgeState: 'error' }))
      .toEqual({ label: '启动失败', tone: 'danger' });
    expect(bridgeRuntimeStatusView({ route: 'local_bridge' }))
      .toEqual({ label: '已停止', tone: 'muted' });
  });

  it('reports a failed status read as unavailable even when last-known state is running', () => {
    expect(bridgeRuntimeStatusView({
      route: 'local_bridge',
      bridgeState: 'running',
      statusUnavailable: true,
    })).toEqual({ label: '状态不可用', tone: 'muted' });
  });
});

describe('adapter profile source resolution', () => {
  const entries = [
    { source: 'account' as const, id: 'row-1', title: 'Account row', agentId: 'codex' as const },
    { source: 'provider' as const, id: 'row-1', title: 'Provider row', agentId: 'kimi' as const },
  ];

  it('matches by (sourceKind, sourceId) so colliding ids resolve correctly', () => {
    expect(resolveAdapterProfileSource(bridgeProfile({ sourceKind: 'provider', sourceId: 'row-1' }), entries))
      .toEqual({ title: 'Provider row', agentId: 'kimi', missing: false });
    expect(resolveAdapterProfileSource(bridgeProfile({ sourceKind: 'account', sourceId: 'row-1' }), entries))
      .toEqual({ title: 'Account row', agentId: 'codex', missing: false });
  });

  it('falls back to the profile name when the source connection is gone', () => {
    const resolved = resolveAdapterProfileSource(bridgeProfile({ sourceId: 'deleted' }), entries);
    expect(resolved).toEqual({ title: 'Kimi → Codex', agentId: null, missing: true });
  });

  it('builds a human-readable flow label for confirmations', () => {
    expect(adapterProfileFlowLabel(bridgeProfile({ sourceKind: 'provider', sourceId: 'row-1' }), entries))
      .toBe('Provider row → Codex');
  });
});

describe('adapter route pipeline', () => {
  it('adds the bridge node only for local_bridge and marks unsupported as broken', () => {
    const bridge = adapterRoutePipelineModel({
      sourceTitle: 'Kimi 会员 Key',
      sourceAgentId: 'kimi',
      credentialLabel: 'API Key',
      targetAgentId: 'codex',
      route: 'local_bridge',
    });
    expect(bridge.nodes.map((node) => node.kind)).toEqual(['source', 'bridge', 'target']);
    expect(bridge.nodes[1].subtitle).toContain('127.0.0.1');
    expect(bridge.broken).toBe(false);

    const direct = adapterRoutePipelineModel({
      sourceTitle: 'Kimi 会员 Key',
      sourceAgentId: 'kimi',
      credentialLabel: 'API Key',
      targetAgentId: 'claude',
      route: 'native_endpoint',
    });
    expect(direct.nodes.map((node) => node.kind)).toEqual(['source', 'target']);
    expect(direct.connectorLabel).toContain('直连');

    const unsupported = adapterRoutePipelineModel({
      sourceTitle: 'Some key',
      credentialLabel: 'API Key',
      targetAgentId: 'grok',
      route: 'unsupported',
    });
    expect(unsupported.broken).toBe(true);
  });

  it('renders pipeline nodes and connector annotation', () => {
    const markup = renderToStaticMarkup(
      createElement(AdapterRoutePipeline, {
        model: adapterRoutePipelineModel({
          sourceTitle: 'Kimi 会员 Key',
          sourceAgentId: 'kimi',
          credentialLabel: 'API Key',
          targetAgentId: 'claude',
          route: 'native_endpoint',
        }),
      }),
    );
    expect(markup).toContain('Kimi 会员 Key');
    expect(markup).toContain('Claude');
    expect(markup).toContain('直连 · 原生端点');
  });

  it('summarizes the path in one line for the confirm dialog', () => {
    expect(adapterApplySummaryLine({ sourceTitle: 'Kimi Key', targetAgentId: 'codex', route: 'local_bridge' }))
      .toBe('Kimi Key → 本地桥接（127.0.0.1） → Codex');
    expect(adapterApplySummaryLine({ sourceTitle: 'Kimi Key', targetAgentId: 'claude', route: 'native_endpoint' }))
      .toBe('Kimi Key → Claude Code');
  });
});

describe('managed adapter profiles view model', () => {
  it('summarizes the bridge fleet and counts degraded listeners as running', () => {
    expect(adapterBridgeFleetSummary([bridgeProfile({ route: 'native_endpoint', localPort: null })], {})).toBeNull();
    const summary = adapterBridgeFleetSummary(
      [bridgeProfile({ id: 'a' }), bridgeProfile({ id: 'b' }), bridgeProfile({ id: 'c' })],
      { a: { state: 'running' }, b: { state: 'degraded' }, c: { state: 'stopped' } },
    );
    expect(summary).toEqual({
      total: 3,
      running: 2,
      label: '3 个本机桥 · 2 个运行中 · 需保持托盘运行',
    });
    expect(adapterBridgeFleetSummary([bridgeProfile()], { 'bridge-1': { state: 'running' } })).toBeNull();
  });

  it('derives the state-matched primary action', () => {
    expect(adapterProfilePrimaryAction({ route: 'native_endpoint' })).toBeNull();
    expect(adapterProfilePrimaryAction({ route: 'local_bridge', bridgeState: 'running' }))
      .toEqual({ kind: 'stop', label: '停止' });
    // A degraded bridge still owns its listener: stop, never start again.
    expect(adapterProfilePrimaryAction({ route: 'local_bridge', bridgeState: 'degraded' }))
      .toEqual({ kind: 'stop', label: '停止' });
    expect(adapterProfilePrimaryAction({ route: 'local_bridge', bridgeState: 'stopped' }))
      .toEqual({ kind: 'start', label: '启动' });
    expect(adapterProfilePrimaryAction({ route: 'local_bridge', bridgeState: 'error' }))
      .toEqual({ kind: 'start', label: '重试启动' });
    expect(adapterProfilePrimaryAction({ route: 'local_bridge', lastErrorCode: 'adapter.bridge_start' }))
      .toEqual({ kind: 'start', label: '重试启动' });
    expect(adapterProfilePrimaryAction({
      route: 'local_bridge',
      bridgeState: 'running',
      statusUnavailable: true,
    })).toEqual({ kind: 'stop', label: '停止' });
    expect(adapterProfilePrimaryAction({
      route: 'local_bridge',
      bridgeState: 'degraded',
      statusUnavailable: true,
    })).toEqual({ kind: 'stop', label: '停止' });
    expect(adapterProfilePrimaryAction({
      route: 'local_bridge',
      statusUnavailable: true,
    })).toEqual({ kind: 'start', label: '启动' });
  });

  it('limits recovery guidance to needs_attention and separates runtime from config repair', () => {
    expect(adapterProfileRecoveryGuide(bridgeProfile({ status: 'active' }))).toBeNull();
    const guide = adapterProfileRecoveryGuide(bridgeProfile({
      status: 'needs_attention',
      lastErrorCode: 'adapter.rollback_incomplete',
    }));
    expect(guide?.summary).toContain('adapter.rollback_incomplete');
    expect(guide?.steps.some((step) => step.includes('不会修复配置不一致'))).toBe(true);
    expect(guide?.steps.some((step) => step.includes('解除绑定后，到 Dashboard 重新连接'))).toBe(true);

    const directGuide = adapterProfileRecoveryGuide({
      route: 'native_endpoint',
      status: 'needs_attention',
      lastErrorCode: null,
    });
    expect(directGuide?.steps.some((step) => step.includes('桥接'))).toBe(false);
  });
});
