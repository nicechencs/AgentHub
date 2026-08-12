import { describe, expect, it, vi } from 'vitest';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import type {
  AdapterAction,
  AdapterApplyPlan,
  AdapterApplyResult,
  AdapterBridgeRuntimeStatus,
  AdapterPlanChange,
  AdapterRouteAnalysis,
} from '@/lib/backend/contracts/adapter';
import {
  adapterActionLabel,
  adapterApplyCommit,
  adapterBridgeEndpointLabel,
  adapterBridgeStateLabel,
  adapterPageViewState,
  adapterPlanChangeLabel,
  adapterProfileRecordLabel,
  adapterProfileStatusLabel,
  canApplyAdapterPlan,
  closeConfirmationOnOpenChange,
  futureAvailability,
  isCurrentAdapterPreviewRequest,
  preventBusyConfirmationDismissal,
  routeLabel,
  sourceLabel,
} from './index';
import { AdapterPreviewResult, isBridgeStopCapable, openAdapterEvidence } from './adapter-components';
import { loadAdapterPageResources, supportBadge } from './adapter-model';
import type { Account, Provider } from '@/lib/types';

const evidence = [{
  label: 'Official evidence',
  url: 'https://example.com/official',
  verifiedAt: '2026-08-12',
}];

function analysis(route: AdapterRouteAnalysis['route']): AdapterRouteAnalysis {
  return {
    route,
    support: route === 'unsupported' ? 'unsupported' : route === 'local_bridge' ? 'experimental' : 'stable',
    reason: 'test route',
    actions: [],
    limitations: [],
    evidence,
  };
}

function plan(route: AdapterRouteAnalysis['route'], changes: AdapterPlanChange[] = []): AdapterApplyPlan {
  return {
    analysis: analysis(route),
    targetAgentId: 'claude',
    canApply: false,
    serviceImpact: route === 'local_bridge' ? 'requires_local_bridge' : 'none',
    changes,
  };
}

describe('Adapter page view model', () => {
  it('routes an empty connection list to the Connections empty state', () => {
    expect(adapterPageViewState({ loading: false, loadError: null, entriesCount: 0, hasSource: false }))
      .toBe('empty');
  });

  it('allows explicit direct plans', () => {
    const native = plan('native_endpoint', [
      { target: 'claude', field: 'baseUrl', value: 'https://api.kimi.com/coding/', secret: false },
      { target: 'claude', field: 'apiKey', secret: true },
    ]);
    expect(routeLabel(native.analysis.route)).toBe('原生端点');
    native.canApply = true;
    expect(canApplyAdapterPlan(native)).toBe(true);
    expect(futureAvailability(native.analysis.route)).toBeNull();
  });

  it('allows an explicit local bridge plan and labels the desktop service impact', () => {
    const local = plan('local_bridge');
    expect(routeLabel(local.analysis.route)).toBe('需要本地代理');
    expect(futureAvailability(local.analysis.route)).toBeNull();
    expect(local.serviceImpact).toBe('requires_local_bridge');
    local.canApply = true;
    expect(canApplyAdapterPlan(local)).toBe(true);
  });

  it('shows unsupported without config writes', () => {
    const unsupported = plan('unsupported');
    expect(routeLabel(unsupported.analysis.route)).toBe('暂未支持');
    expect(supportBadge(unsupported.analysis.support).label).toBe('暂未支持');
    expect(futureAvailability(unsupported.analysis.route)).toBeNull();
    expect(unsupported.changes).toEqual([]);
    expect(canApplyAdapterPlan(unsupported)).toBe(false);
  });

  it('renders an actionable unsupported state without mutation controls', () => {
    const unsupported = plan('unsupported');
    unsupported.analysis.reason = '当前尚未完成上游授权、条款和协议兼容性验证。';
    const markup = renderToStaticMarkup(
      <AdapterPreviewResult
        analysis={unsupported.analysis}
        plan={unsupported}
        loading={false}
        error={null}
        onRetry={vi.fn()}
        onApply={vi.fn()}
      />,
    );
    expect(markup).toContain('暂未支持此组合');
    expect(markup).toContain('暂未支持不等于连接失效');
    expect(markup).toContain('改用目标 Agent 自身登录');
    expect(markup).not.toContain('应用配置');
    expect(markup).not.toContain('启用本地桥接');
    expect(markup).not.toContain('无需本地服务');
  });

  it('opens compatibility evidence through the injected external opener', async () => {
    const opener = vi.fn().mockResolvedValue(undefined);
    await openAdapterEvidence(evidence[0].url, opener);
    expect(opener).toHaveBeenCalledWith(evidence[0].url);

    const failure = new Error('system browser unavailable');
    await expect(openAdapterEvidence(evidence[0].url, vi.fn().mockRejectedValue(failure)))
      .rejects.toBe(failure);
  });

  it('marks preview-only config_sync as future availability', () => {
    const configSync = plan('config_sync');
    expect(futureAvailability(configSync.analysis.route)).toBe('配置写入后续开放');
  });

  it('clears an old preview response when a newer selection is in flight', () => {
    expect(isCurrentAdapterPreviewRequest(3, 4)).toBe(false);
    expect(isCurrentAdapterPreviewRequest(4, 4)).toBe(true);
  });

  it('never renders a secret action or change value', () => {
    const unsafeWireChange = {
      target: 'claude', field: 'apiKey', value: 'sk-visible-secret', secret: true,
    } as unknown as AdapterPlanChange;
    const unsafeWireAction = {
      kind: 'reference_connection_secret',
      target: 'Claude Code',
      description: 'Use selected connection',
      value: 'sk-visible-secret',
      secret: true,
    } as unknown as AdapterAction;
    expect(adapterPlanChangeLabel(unsafeWireChange)).not.toContain('sk-visible-secret');
    expect(adapterActionLabel(unsafeWireAction)).not.toContain('sk-visible-secret');
  });

  it('labels saved profiles with their source, target, and lifecycle status', () => {
    const profile = {
      id: 'adapter-1',
      name: 'Kimi → Claude',
      sourceKind: 'provider' as const,
      sourceId: 'kimi-connection',
      targetAgentId: 'claude' as const,
      route: 'native_endpoint' as const,
      status: 'active' as const,
      ruleId: 'rule',
      ruleVersion: '1',
      generatedProviderId: 'claude-kimi-adapter',
      localPort: null,
      autoStart: false,
      createdAt: '2026-08-12T00:00:00Z',
      updatedAt: '2026-08-12T00:00:00Z',
    };
    expect(adapterProfileRecordLabel(profile)).toBe('Provider · …tion → Claude Code');
    expect(adapterProfileStatusLabel(profile.status)).toBe('已生效');
    expect(adapterProfileStatusLabel('needs_attention')).toBe('需要处理');
  });

  it('renders bridge runtime labels and only loopback endpoint information', () => {
    const profile = {
      id: 'bridge-1', name: 'Kimi → Codex', sourceKind: 'provider' as const, sourceId: 'kimi-1',
      targetAgentId: 'codex' as const, route: 'local_bridge' as const, status: 'active' as const,
      ruleId: 'bridge', ruleVersion: '1', generatedProviderId: 'codex-bridge-1', localPort: 32123,
      autoStart: true, createdAt: '2026-08-12T00:00:00Z', updatedAt: '2026-08-12T00:00:00Z',
    };
    const runtime: AdapterBridgeRuntimeStatus = {
      profileId: profile.id, state: 'running', port: 32123,
      endpoint: 'http://127.0.0.1:32123/v1', startedAt: '2026-08-12T00:00:00Z', upstreamStatus: 'connected',
    };
    expect(adapterBridgeStateLabel(runtime.state)).toBe('运行中');
    expect(adapterBridgeEndpointLabel(profile, runtime)).toBe('127.0.0.1:32123');
    expect(JSON.stringify({ runtime, endpoint: adapterBridgeEndpointLabel(profile, runtime) })).not.toContain('token');
  });

  it('uses one canonical source label with kind, current state, and a stable masked id suffix', () => {
    const label = sourceLabel({
      source: 'account', id: 'account-1234', agentId: 'claude', title: 'Work OAuth', isCurrent: true,
    });
    expect(label).toBe('账户 · Claude Code · Work OAuth · 当前 · …1234');
  });

  it('preserves successful resources when another pool or bridge status fails', async () => {
    const profile = {
      id: 'bridge-1', name: 'Bridge', sourceKind: 'provider' as const, sourceId: 'source-9876',
      targetAgentId: 'codex' as const, route: 'local_bridge' as const, status: 'active' as const,
      ruleId: 'bridge', ruleVersion: '1', generatedProviderId: 'codex-bridge-1', localPort: 32123,
      autoStart: true, createdAt: '2026-08-12T00:00:00Z', updatedAt: '2026-08-12T00:00:00Z',
    };
    const account: Account = {
      id: 'account-1234', agentId: 'claude', kind: 'apikey', label: 'Work key', isCurrent: true, tokenValid: true,
    };
    const result = await loadAdapterPageResources({
      listAccounts: async () => [account],
      listProviders: async () => Promise.reject(new Error('provider unavailable')) as Promise<Provider[]>,
      listProfiles: async () => [profile],
      getBridgeStatus: async () => Promise.reject(new Error('bridge unavailable')),
    });

    expect(result.connectionState).toBe('partial');
    expect(result.entries).toHaveLength(1);
    expect(result.profiles).toEqual([profile]);
    expect(result.errors.providers).toBeInstanceOf(Error);
    expect(result.errors.bridgeStatuses[profile.id]).toBeInstanceOf(Error);
    expect(result.bridgeStatuses[profile.id]).toMatchObject({ state: 'error', upstreamStatus: 'unavailable' });
  });

  it('reports a failed profile request instead of treating it as an empty profile list', async () => {
    const result = await loadAdapterPageResources({
      listAccounts: async () => [],
      listProviders: async () => [],
      listProfiles: async () => Promise.reject(new Error('profiles unavailable')) as Promise<never[]>,
      getBridgeStatus: async () => ({ profileId: 'unused', state: 'stopped' }),
    });

    expect(result.profileState).toBe('error');
    expect(result.errors.profiles).toBeInstanceOf(Error);
    expect(result.profiles).toEqual([]);
  });

  it('commits apply success before deciding whether to probe bridge runtime state', () => {
    const result: Pick<AdapterApplyResult, 'profile'> = {
      profile: {
        id: 'adapter-1', name: 'Direct', sourceKind: 'account', sourceId: 'account-1234', targetAgentId: 'claude',
        route: 'native_endpoint', status: 'active', ruleId: 'direct', ruleVersion: '1', generatedProviderId: null,
        localPort: null, autoStart: false, createdAt: '2026-08-12T00:00:00Z', updatedAt: '2026-08-12T00:00:00Z',
      },
    };
    expect(adapterApplyCommit(result)).toEqual({
      successMessage: '适配已应用。', shouldProbeBridge: false, shouldRefresh: true,
    });
  });
});

describe('Adapter profile interactions', () => {
  it('keeps Apply, Stop and Delete confirmations open when Radix reports an attempted busy close', () => {
    const closeApply = vi.fn();
    const closeStop = vi.fn();
    const closeDelete = vi.fn();

    closeConfirmationOnOpenChange(false, true, closeApply);
    closeConfirmationOnOpenChange(false, true, closeStop);
    closeConfirmationOnOpenChange(false, true, closeDelete);

    expect(closeApply).not.toHaveBeenCalled();
    expect(closeStop).not.toHaveBeenCalled();
    expect(closeDelete).not.toHaveBeenCalled();
  });

  it('prevents Escape and outside-pointer dismissal while a confirmation mutation is busy', () => {
    const preventEscape = vi.fn();
    const preventOutsidePointer = vi.fn();

    preventBusyConfirmationDismissal(true, { preventDefault: preventEscape });
    preventBusyConfirmationDismissal(true, { preventDefault: preventOutsidePointer });

    expect(preventEscape).toHaveBeenCalledOnce();
    expect(preventOutsidePointer).toHaveBeenCalledOnce();
  });

  it('allows a confirmation to close after its operation has settled', () => {
    const close = vi.fn();
    const preventDefault = vi.fn();

    closeConfirmationOnOpenChange(false, false, close);
    preventBusyConfirmationDismissal(false, { preventDefault });

    expect(close).toHaveBeenCalledOnce();
    expect(preventDefault).not.toHaveBeenCalled();
  });

  it('keeps a degraded bridge stop-capable instead of offering Start', () => {
    expect(isBridgeStopCapable('running')).toBe(true);
    expect(isBridgeStopCapable('degraded')).toBe(true);
    expect(isBridgeStopCapable('starting')).toBe(false);
    expect(isBridgeStopCapable('stopped')).toBe(false);
  });
});
