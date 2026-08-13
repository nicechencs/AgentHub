import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
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
  adapterBridgeUpstreamLabel,
  adapterPageViewState,
  adapterPlanChangeLabel,
  adapterProfileRecordLabel,
  adapterProfileStatusLabel,
  canApplyAdapterPlan,
  closeConfirmationOnOpenChange,
  futureAvailability,
  isCurrentAdapterPreviewRequest,
  isSubscriptionGateUnsupported,
  preventBusyConfirmationDismissal,
  routeLabel,
  sourceLabel,
  sourceStatusHint,
  unsupportedPresentation,
} from './index';
import { AdapterPreviewResult, isBridgeStopCapable, openAdapterEvidence } from './adapter-components';
import { startAdapterBridgeStatusPoll } from './use-adapter-resources';
import {
  ADAPTER_BRIDGE_STATUS_POLL_MS,
  adapterBridgeProfilesToPoll,
  applyAdapterBridgeStatusPoll,
  loadAdapterPageResources,
  shouldPollAdapterBridgeStatus,
  supportBadge,
} from './adapter-model';
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
    expect(routeLabel(unsupported.analysis.route)).toBe('当前不支持');
    expect(supportBadge(unsupported.analysis.support).label).toBe('当前不支持');
    expect(futureAvailability(unsupported.analysis.route)).toBeNull();
    expect(unsupported.changes).toEqual([]);
    expect(canApplyAdapterPlan(unsupported)).toBe(false);
  });

  it('renders an actionable unsupported state without mutation controls', () => {
    const unsupported = plan('unsupported');
    unsupported.analysis.reason = '当前尚未完成上游授权、条款和协议兼容性验证。';
    // Prefer createElement over JSX so the test stays valid under both classic and
    // automatic JSX runtimes (typecheck noUnusedLocals + vitest runtime).
    const markup = renderToStaticMarkup(
      createElement(AdapterPreviewResult, {
        analysis: unsupported.analysis,
        plan: unsupported,
        loading: false,
        error: null,
        onRetry: vi.fn(),
        onApply: vi.fn(),
      }),
    );
    expect(markup).toContain('暂未支持此组合');
    expect(markup).toContain('门禁说明');
    expect(markup).toContain('plan.canApply=false');
    expect(markup).toContain('可用替代路径');
    expect(markup).toContain('改用目标 Agent 自身登录');
    expect(markup).not.toContain('应用配置');
    expect(markup).not.toContain('启用本地桥接');
    expect(markup).not.toContain('无需本地服务');
    expect(markup).not.toMatch(/<button[^>]*>[\s\S]*强制继续/);
  });

  it('marks Codex/ChatGPT → Claude as gated unsupported with alternatives and no apply path', () => {
    const unsupported = plan('unsupported');
    unsupported.analysis.reason = [
      'Codex / ChatGPT 订阅 → Claude Code：当前不支持。',
      '尚未通过上游授权、条款与协议兼容性门禁，plan.canApply=false。',
    ].join('');
    unsupported.analysis.gateKind = 'subscription_candidate';
    unsupported.analysis.ruleId = 'codex-subscription-to-claude-app-server-v0';
    unsupported.analysis.evidence = [{
      label: 'Codex / ChatGPT subscription → Claude Code 门禁',
      url: 'https://github.com/nicechencs/AgentHub/blob/release/docs/provider-api-oauth-adaptation.md#51-codex--chatgpt-subscription--claude-code当前结论与前置门禁',
      verifiedAt: '2026-08-12',
    }];
    expect(isSubscriptionGateUnsupported(unsupported.analysis)).toBe(true);
    expect(isSubscriptionGateUnsupported({
      route: 'unsupported',
      reason: 'generic missing rule',
      evidence: [],
      gateKind: 'subscription_candidate',
    })).toBe(true);
    expect(canApplyAdapterPlan(unsupported)).toBe(false);
    const presentation = unsupportedPresentation(unsupported.analysis, unsupported);
    expect(presentation.headline).toBe('当前不支持');
    expect(presentation.canApply).toBe(false);
    expect(presentation.alternatives.some((line) => line.includes('Claude'))).toBe(true);
    expect(presentation.alternatives.some((line) => /API Key/i.test(line))).toBe(true);

    const markup = renderToStaticMarkup(
      createElement(AdapterPreviewResult, {
        analysis: unsupported.analysis,
        plan: unsupported,
        loading: false,
        error: null,
        onRetry: vi.fn(),
        // Even if a caller accidentally supplies onApply, unsupported must not render it.
        onApply: vi.fn(),
      }),
    );
    expect(markup).toContain('当前不支持');
    expect(markup).toContain('门禁说明');
    expect(markup).toContain('plan.canApply=false');
    expect(markup).toContain('也没有“强制继续”');
    expect(markup).toContain('Claude');
    expect(markup).toContain('API Key');
    // Mutation controls must not appear as buttons/actions (explanatory copy may mention them).
    expect(markup).not.toContain('应用配置');
    expect(markup).not.toContain('启用本地桥接');
    expect(markup).not.toMatch(/<button[^>]*>[\s\S]*强制继续/);
  });

  it('labels source OAuth/API Key health without credential material', () => {
    expect(sourceStatusHint({
      kind: 'oauth',
      authHealth: 'verified',
      authStatus: 'valid',
    })).toContain('官方登录');
    expect(sourceStatusHint({
      kind: 'oauth',
      authHealth: 'needs_login',
      authStatus: 'expired',
    })).toContain('继续授权');
    expect(sourceStatusHint({
      kind: 'apikey',
      authHealth: 'configured',
      authStatus: 'valid',
    })).toContain('API Key');
    expect(sourceStatusHint({
      kind: 'oauth',
      authHealth: 'verified',
      authStatus: 'valid',
    })).not.toMatch(/sk-|token|secret|bearer/i);
  });

  it('renders loading and error preview states with Chinese guidance', () => {
    const loadingMarkup = renderToStaticMarkup(
      createElement(AdapterPreviewResult, {
        analysis: null,
        plan: null,
        loading: true,
        error: null,
        onRetry: vi.fn(),
      }),
    );
    expect(loadingMarkup).toContain('正在分析路径并生成只读配置预览');
    expect(loadingMarkup).toContain('connectionId');

    const errorMarkup = renderToStaticMarkup(
      createElement(AdapterPreviewResult, {
        analysis: null,
        plan: null,
        loading: false,
        error: new Error('network down'),
        onRetry: vi.fn(),
      }),
    );
    expect(errorMarkup).toContain('无法生成适配预览');
    expect(errorMarkup).toContain('不是连接失效');
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
    expect(adapterBridgeUpstreamLabel(runtime.upstreamStatus)).toBe('已连接');
    expect(adapterBridgeUpstreamLabel('stopped')).toBe('已停止');
    expect(adapterBridgeUpstreamLabel('degraded')).toBe('降级');
    expect(adapterBridgeUpstreamLabel('unavailable')).toBe('不可用');
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

  it('polls only running or degraded local-bridge profiles and clears a stale generation', () => {
    const running = {
      id: 'bridge-running', name: 'Running', sourceKind: 'provider' as const, sourceId: 'source-1',
      targetAgentId: 'codex' as const, route: 'local_bridge' as const, status: 'active' as const,
      ruleId: 'bridge', ruleVersion: '1', generatedProviderId: 'codex-bridge-1', localPort: 32123,
      autoStart: true, createdAt: '2026-08-12T00:00:00Z', updatedAt: '2026-08-12T00:00:00Z',
    };
    const stopped = { ...running, id: 'bridge-stopped' };
    const native = { ...running, id: 'native-1', route: 'native_endpoint' as const, localPort: null };
    const statuses = {
      [running.id]: { profileId: running.id, state: 'running' as const, port: 32123, upstreamStatus: 'connected' },
      [stopped.id]: { profileId: stopped.id, state: 'stopped' as const, port: 32123, upstreamStatus: 'stopped' },
    };
    expect(shouldPollAdapterBridgeStatus(running, statuses[running.id])).toBe(true);
    expect(shouldPollAdapterBridgeStatus(running, { ...statuses[running.id], state: 'degraded' })).toBe(true);
    expect(shouldPollAdapterBridgeStatus(stopped, statuses[stopped.id])).toBe(false);
    expect(shouldPollAdapterBridgeStatus(native, statuses[running.id])).toBe(false);
    expect(adapterBridgeProfilesToPoll([running, stopped, native], statuses).map((item) => item.id)).toEqual([running.id]);
    expect(ADAPTER_BRIDGE_STATUS_POLL_MS).toBeGreaterThanOrEqual(3_000);
    expect(ADAPTER_BRIDGE_STATUS_POLL_MS).toBeLessThanOrEqual(5_000);

    const current = {
      entries: [],
      profiles: [running],
      bridgeStatuses: statuses,
      errors: { bridgeStatuses: {} },
      connectionState: 'ready' as const,
      profileState: 'ready' as const,
    };
    const next = applyAdapterBridgeStatusPoll(current, [running], [{
      status: 'fulfilled',
      value: { profileId: running.id, state: 'degraded', port: 32123, upstreamStatus: 'degraded' },
    }]);
    expect(next.bridgeStatuses[running.id]).toMatchObject({ state: 'degraded', upstreamStatus: 'degraded' });

    const failed = applyAdapterBridgeStatusPoll(current, [running], [{
      status: 'rejected',
      reason: new Error('status read failed'),
    }]);
    expect(failed.bridgeStatuses[running.id]).toMatchObject({ state: 'error', upstreamStatus: 'unavailable' });
    expect(failed.errors.bridgeStatuses[running.id]).toBeInstanceOf(Error);
    expect(isCurrentAdapterPreviewRequest(1, 2)).toBe(false);

    let generation = 1;
    const setIntervalFn = vi.fn().mockReturnValue(77);
    const clearIntervalFn = vi.fn();
    const stop = startAdapterBridgeStatusPoll({
      getGeneration: () => generation,
      getResources: () => current,
      apply: vi.fn(),
      getBridgeStatus: async () => {
        throw new Error('poll must not run after dispose');
      },
      setIntervalFn: setIntervalFn as unknown as typeof setInterval,
      clearIntervalFn: clearIntervalFn as unknown as typeof clearInterval,
    });
    expect(setIntervalFn).toHaveBeenCalledWith(expect.any(Function), ADAPTER_BRIDGE_STATUS_POLL_MS);
    generation += 1;
    stop();
    expect(clearIntervalFn).toHaveBeenCalledWith(77);
  });
});
