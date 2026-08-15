import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import {
  adapterCommandError,
  type AdapterAction,
  type AdapterApplyPlan,
  type AdapterApplyResult,
  type AdapterBridgeRuntimeStatus,
  type AdapterPlanChange,
  type AdapterRouteAnalysis,
} from '@/lib/backend/contracts/adapter';
import {
  adapterActionLabel,
  adapterAgentBadgeStyle,
  adapterApplyCommit,
  adapterBridgeEndpointLabel,
  adapterBridgeStateLabel,
  adapterBridgeUpstreamLabel,
  adapterPageDescription,
  adapterPageViewState,
  adapterPlanChangeLabel,
  adapterPlanRequestSignature,
  adapterPreviewOutcome,
  adapterProfileRecordLabel,
  adapterProfileStatusLabel,
  adapterServiceImpactLabel,
  adapterTableRouteLabel,
  adapterTabLabel,
  adapterCredentialFilterLabel,
  adapterCredentialKindLabel,
  canApplyAdapterPlan,
  canApplyAdapterSelection,
  canConfirmAdapterApply,
  canRequestAdapterPlan,
  closeConfirmationOnOpenChange,
  connectionKindForTab,
  filterProfilesByCredential,
  filterProfilesByMode,
  parseAdapterCredentialFilter,
  parseAdapterTab,
  resolveAdapterTargetAgentId,
  resolveAdapterVisibleSourceKey,
  isAdapterPlanMatchedToSelection,
  isCurrentAdapterPreviewRequest,
  isSameAdapterPlanRequestSignature,
  isSubscriptionGateUnsupported,
  preventBusyConfirmationDismissal,
  routeLabel,
  sourceLabel,
  sourceStatusHint,
  unsupportedPresentation,
} from './index';
import { AdapterPreviewResult, AdapterProfiles, isBridgeStopCapable, openAdapterEvidence } from './adapter-components';
import { startAdapterBridgeStatusPoll } from './use-adapter-resources';
import {
  ADAPTER_BRIDGE_STATUS_POLL_MS,
  adapterBridgeProfilesToPoll,
  adapterErrorDetails,
  applyAdapterBridgeStatusPoll,
  errorMessage,
  isAdapterErrorRetryable,
  loadAdapterPageResources,
  loadAdapterProfileResources,
  mergeAdapterProfileLoad,
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
  it('describes the page as profile and bridge management', () => {
    expect(adapterPageDescription()).toBe(
      '日常连接与跨服务复用请走 Dashboard「连接/切换」或 Connections「用于其他 Agent」。本页管理已创建的适配与本地桥。',
    );
  });

  it('routes an empty connection list to the Connections empty state', () => {
    expect(adapterPageViewState({ loading: false, loadError: null, entriesCount: 0, hasSource: false }))
      .toBe('empty');
  });

  it('maps legacy ?tab= links onto a credential filter and keeps local_bridge off OAuth', () => {
    expect(parseAdapterCredentialFilter(null)).toBe('all');
    expect(parseAdapterCredentialFilter('oauth')).toBe('oauth');
    // Legacy wire token `api` normalizes to shared `apikey`.
    expect(parseAdapterCredentialFilter('api')).toBe('apikey');
    expect(parseAdapterCredentialFilter('apikey')).toBe('apikey');
    expect(parseAdapterTab('bridge')).toBe('all');
    expect(connectionKindForTab('apikey')).toBe('apikey');
    expect(connectionKindForTab('oauth')).toBe('oauth');
    expect(adapterTabLabel('apikey')).toBe('API Key');
    expect(adapterCredentialFilterLabel('all')).toBe('全部');
    expect(adapterCredentialKindLabel('oauth')).toBe('官方登录');
    expect(filterProfilesByMode([
      { mode: 'api' },
      { mode: 'oauth' },
      { mode: 'api' },
    ], 'api')).toHaveLength(2);
    expect(filterProfilesByCredential([
      { mode: 'api' },
      { mode: 'oauth' },
    ], 'all')).toHaveLength(2);
    expect(filterProfilesByCredential([
      { mode: 'api' },
      { mode: 'oauth' },
    ], 'oauth')).toHaveLength(1);
    expect(filterProfilesByCredential([
      { mode: 'api' },
      { mode: 'oauth' },
    ], 'apikey')).toHaveLength(1);
    expect(routeLabel('local_bridge')).toBe('需要本地代理');
    expect(adapterTableRouteLabel('local_bridge')).toBe('本地协议转换');
    expect(adapterTableRouteLabel('native_endpoint')).toBe('原生端点');
  });

  it('allows explicit direct plans', () => {
    const native = plan('native_endpoint', [
      { target: 'claude', field: 'baseUrl', value: 'https://api.kimi.com/coding/', secret: false },
      { target: 'claude', field: 'apiKey', secret: true },
    ]);
    expect(routeLabel(native.analysis.route)).toBe('原生端点');
    native.canApply = true;
    expect(canApplyAdapterPlan(native)).toBe(true);
  });

  it('allows an explicit local bridge plan and labels the desktop service impact', () => {
    const local = plan('local_bridge');
    expect(routeLabel(local.analysis.route)).toBe('需要本地代理');
    expect(local.serviceImpact).toBe('requires_local_bridge');
    local.canApply = true;
    expect(canApplyAdapterPlan(local)).toBe(true);
  });

  it('shows unsupported without config writes', () => {
    const unsupported = plan('unsupported');
    expect(routeLabel(unsupported.analysis.route)).toBe('当前不支持');
    expect(supportBadge(unsupported.analysis.support).label).toBe('当前不支持');
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
    expect(markup).toContain('不能应用');
    expect(markup).toContain('改用目标 Agent 自己登录');
    expect(markup).toContain('查看详情');
    expect(markup).not.toContain('plan.canApply');
    expect(markup).not.toContain('应用配置');
    expect(markup).not.toContain('启用本地桥接');
    expect(markup).not.toMatch(/<button[^>]*>[\s\S]*强制继续/);
  });

  it('marks Codex/ChatGPT → Claude as gated unsupported with alternatives and no apply path', () => {
    const unsupported = plan('unsupported');
    unsupported.analysis.reason = [
      'Codex / ChatGPT 订阅 → Claude Code：当前不支持。',
      '尚未通过上游授权、条款与协议兼容性验证。',
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
    expect(presentation.gateLines.join('\n')).not.toContain('plan.canApply');

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
    expect(markup).toContain('Claude');
    expect(markup).toContain('API Key');
    expect(markup).toContain('查看详情');
    expect(markup).not.toContain('plan.canApply');
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
    })).toBe('已验证');
    expect(sourceStatusHint({
      kind: 'oauth',
      authHealth: 'needs_login',
      authStatus: 'expired',
    })).toContain('重新登录');
    expect(sourceStatusHint({
      kind: 'apikey',
      authHealth: 'configured',
      authStatus: 'valid',
    })).toContain('已配置');
    expect(sourceStatusHint({
      kind: 'oauth',
      authHealth: 'verified',
      authStatus: 'valid',
    })).not.toMatch(/sk-|token|secret|bearer|connectionId/i);
  });

  it('presents applyable routes with plain-language outcome instead of internal flags', () => {
    expect(adapterPreviewOutcome({
      route: 'native_endpoint',
      canApply: true,
    })).toMatchObject({
      title: '可接入 · 直接写入',
      badgeLabel: '可应用',
    });
    expect(adapterPreviewOutcome({
      route: 'local_bridge',
      canApply: true,
    }).nextStep).toContain('本机桥接');
    expect(adapterPreviewOutcome({
      route: 'config_sync',
      canApply: true,
    })).toMatchObject({
      title: '可接入 · 直接写入',
      badgeLabel: '可应用',
      nextStep: '确认后写入目标配置。',
    });
    expect(adapterPreviewOutcome({
      route: 'config_sync',
      canApply: false,
    }).badgeLabel).toBe('仅预览');
    expect(adapterServiceImpactLabel('requires_local_bridge')).toContain('本机桥接');
    expect(adapterServiceImpactLabel('none')).toBe('无需本地服务');

    const applyable = {
      ...plan('native_endpoint', [
        { target: 'claude', field: 'baseUrl', value: 'https://api.kimi.com/coding/', secret: false },
      ]),
      canApply: true,
    };
    const markup = renderToStaticMarkup(
      createElement(AdapterPreviewResult, {
        analysis: applyable.analysis,
        plan: applyable,
        loading: false,
        error: null,
        onRetry: vi.fn(),
        onApply: vi.fn(),
      }),
    );
    expect(markup).toContain('可接入 · 直接写入');
    expect(markup).toContain('应用配置');
    expect(markup).toContain('查看详情');
    expect(markup).toContain('预计改动');
    expect(markup).not.toContain('plan.canApply');
    expect(markup).not.toContain('稳定规则');

    const syncApplyable = { ...plan('config_sync'), canApply: true, targetAgentId: 'pi' as const };
    const syncMarkup = renderToStaticMarkup(
      createElement(AdapterPreviewResult, {
        analysis: syncApplyable.analysis,
        plan: syncApplyable,
        loading: false,
        error: null,
        onRetry: vi.fn(),
        onApply: vi.fn(),
      }),
    );
    expect(syncMarkup).toContain('可接入 · 直接写入');
    expect(syncMarkup).toContain('应用配置');
    expect(syncMarkup).not.toContain('仅预览');
    expect(syncMarkup).not.toContain('配置写入后续开放');
  });

  it('styles agent badges from brand CSS vars without inventing hex colors', () => {
    const style = adapterAgentBadgeStyle('var(--agent-claude)');
    expect(style.color).toBe('var(--agent-claude)');
    expect(style.backgroundColor).toContain('var(--agent-claude)');
    expect(style.backgroundColor).toContain('color-mix');
    expect(style.boxShadow).toContain('var(--agent-claude)');
    expect(JSON.stringify(style)).not.toMatch(/#[0-9a-fA-F]{3,8}/);
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
    expect(loadingMarkup).toContain('分析中');
    expect(loadingMarkup).not.toContain('connectionId');

    const errorMarkup = renderToStaticMarkup(
      createElement(AdapterPreviewResult, {
        analysis: null,
        plan: null,
        loading: false,
        error: new Error('network down'),
        onRetry: vi.fn(),
      }),
    );
    expect(errorMarkup).toContain('分析失败');
  });

  it('prefers AdapterCommandError message and classifies retryable helpers', () => {
    const retryable = adapterCommandError({
      code: 'adapter.port_in_use',
      message: '本地端口被占用',
      details: '127.0.0.1:32123 already bound',
    });
    const rollback = adapterCommandError({
      code: 'adapter.bridge_rollback',
      message: '回滚未完成',
    });
    expect(errorMessage(retryable, 'fallback')).toBe('本地端口被占用');
    expect(errorMessage({ message: '' }, 'fallback')).toBe('fallback');
    expect(errorMessage('legacy string error', 'fallback')).toBe('legacy string error');
    expect(isAdapterErrorRetryable(retryable)).toBe(true);
    expect(isAdapterErrorRetryable(rollback)).toBe(false);
    expect(isAdapterErrorRetryable({ code: 'adapter.bridge_restore_source' })).toBe(true);
    expect(isAdapterErrorRetryable({ code: 'needs_attention' })).toBe(false);
    expect(isAdapterErrorRetryable({ code: 'adapter.command' })).toBe(false);
    expect(isAdapterErrorRetryable(new Error('plain'))).toBe(false);
    expect(adapterErrorDetails(retryable)).toBe('127.0.0.1:32123 already bound');
    expect(adapterErrorDetails(rollback)).toBeNull();
  });

  it('shows a retryable hint on apply and profile-row Adapter errors', () => {
    const applyMarkup = renderToStaticMarkup(
      createElement(AdapterPreviewResult, {
        analysis: analysis('native_endpoint'),
        plan: { ...plan('native_endpoint'), canApply: true },
        loading: false,
        error: null,
        onRetry: vi.fn(),
        onApply: vi.fn(),
        applyError: adapterCommandError({
          code: 'adapter.bridge_start',
          message: '本地桥接启动失败',
          details: 'listener bind failed',
        }),
      }),
    );
    expect(applyMarkup).toContain('本地桥接启动失败');
    expect(applyMarkup).toContain('listener bind failed');
    expect(applyMarkup).toContain('此错误可重试');

    const profile = {
      id: 'bridge-1',
      name: 'Kimi → Codex',
      sourceKind: 'provider' as const,
      sourceId: 'kimi-1',
      targetAgentId: 'codex' as const,
      route: 'local_bridge' as const,
      mode: 'api' as const,
      status: 'active' as const,
      ruleId: 'bridge',
      ruleVersion: '1',
      generatedProviderId: 'codex-bridge-1',
      localPort: 32123,
      autoStart: true,
      createdAt: '2026-08-12T00:00:00Z',
      updatedAt: '2026-08-12T00:00:00Z',
    };
    const rowMarkup = renderToStaticMarkup(
      createElement(AdapterProfiles, {
        profiles: [profile],
        bridgeStatuses: {},
        statusErrors: {},
        entries: [],
        loading: false,
        loadError: null,
        errors: {
          [profile.id]: adapterCommandError({
            code: 'adapter.port_in_use',
            message: '端口被占用',
          }),
        },
        removingProfileId: null,
        busyProfileIds: {},
        onStartBridge: vi.fn(),
        onRequestStopBridge: vi.fn(),
        onShowDetail: vi.fn(),
        onRetry: vi.fn(),
        onStartCreate: vi.fn(),
      }),
    );
    expect(rowMarkup).toContain('端口被占用');
    expect(rowMarkup).toContain('此错误可重试');
  });

  it('opens compatibility evidence through the injected external opener', async () => {
    const opener = vi.fn().mockResolvedValue(undefined);
    await openAdapterEvidence(evidence[0].url, opener);
    expect(opener).toHaveBeenCalledWith(evidence[0].url);

    const failure = new Error('system browser unavailable');
    await expect(openAdapterEvidence(evidence[0].url, vi.fn().mockRejectedValue(failure)))
      .rejects.toBe(failure);
  });

  it('clears an old preview response when a newer selection is in flight', () => {
    expect(isCurrentAdapterPreviewRequest(3, 4)).toBe(false);
    expect(isCurrentAdapterPreviewRequest(4, 4)).toBe(true);
  });

  it('binds plan preview and apply to the exact source/target signature', () => {
    const signatureA = adapterPlanRequestSignature({
      sourceKind: 'provider',
      sourceId: 'kimi-1',
      targetAgentId: 'claude',
    });
    const signatureB = adapterPlanRequestSignature({
      sourceKind: 'provider',
      sourceId: 'kimi-2',
      targetAgentId: 'claude',
    });
    const signatureTargetSwap = adapterPlanRequestSignature({
      sourceKind: 'provider',
      sourceId: 'kimi-1',
      targetAgentId: 'codex',
    });
    const applyable = { ...plan('native_endpoint'), canApply: true };

    expect(isSameAdapterPlanRequestSignature(signatureA, {
      sourceKind: 'provider',
      sourceId: 'kimi-1',
      targetAgentId: 'claude',
    })).toBe(true);
    expect(isSameAdapterPlanRequestSignature(signatureA, signatureB)).toBe(false);
    expect(isSameAdapterPlanRequestSignature(signatureA, signatureTargetSwap)).toBe(false);

    // Old plan from A must not preview or apply after switching to B.
    expect(isAdapterPlanMatchedToSelection(applyable, signatureA, signatureB)).toBe(false);
    expect(canApplyAdapterSelection({
      plan: applyable,
      planSignature: signatureA,
      currentSignature: signatureB,
    })).toBe(false);
    expect(canConfirmAdapterApply({
      applyRequest: signatureB,
      plan: applyable,
      planSignature: signatureA,
    })).toBe(false);

    // Target switch is the same class of mismatch.
    expect(canApplyAdapterSelection({
      plan: applyable,
      planSignature: signatureA,
      currentSignature: signatureTargetSwap,
    })).toBe(false);

    // Matching signature + backend gate still required.
    expect(canApplyAdapterSelection({
      plan: applyable,
      planSignature: signatureA,
      currentSignature: signatureA,
    })).toBe(true);
    expect(canApplyAdapterSelection({
      plan: { ...applyable, canApply: false },
      planSignature: signatureA,
      currentSignature: signatureA,
    })).toBe(false);
    expect(canApplyAdapterSelection({
      plan: applyable,
      planSignature: signatureA,
      currentSignature: signatureA,
      authIncomplete: true,
    })).toBe(false);
    expect(canConfirmAdapterApply({
      applyRequest: signatureA,
      plan: applyable,
      planSignature: signatureA,
    })).toBe(true);
    expect(canConfirmAdapterApply({
      applyRequest: null,
      plan: applyable,
      planSignature: signatureA,
    })).toBe(false);

    // Late response for generation N is still discarded when N+1 is current.
    expect(isCurrentAdapterPreviewRequest(1, 2)).toBe(false);
    expect(isAdapterPlanMatchedToSelection(applyable, signatureA, signatureA)).toBe(true);
  });

  it('clears selection when the current source is filtered or searched out of the visible list', () => {
    const visible = [
      { key: 'provider:kimi-1' },
      { key: 'account:codex-1' },
    ];
    expect(resolveAdapterVisibleSourceKey('provider:kimi-1', visible)).toBe('provider:kimi-1');
    // Switching 全部/API Key → 官方登录 (or a search that hides the row) must drop selection.
    expect(resolveAdapterVisibleSourceKey('provider:kimi-1', [{ key: 'account:codex-1' }])).toBe('');
    expect(resolveAdapterVisibleSourceKey('provider:kimi-1', [])).toBe('');
    expect(resolveAdapterVisibleSourceKey('', visible)).toBe('');
  });

  it('does not reuse a stale target Agent when none are selectable', () => {
    expect(resolveAdapterTargetAgentId('claude', [])).toBe('');
    expect(resolveAdapterTargetAgentId('claude', ['codex', 'pi'])).toBe('codex');
    expect(resolveAdapterTargetAgentId('pi', ['codex', 'pi'])).toBe('pi');
    expect(canRequestAdapterPlan({ sourceId: 'src-1', targetAgentId: '' })).toBe(false);
    expect(canRequestAdapterPlan({ sourceId: 'src-1', targetAgentId: 'claude' })).toBe(true);
    expect(canRequestAdapterPlan({ sourceId: '', targetAgentId: 'claude' })).toBe(false);
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
      mode: 'api' as const,
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
      targetAgentId: 'codex' as const, route: 'local_bridge' as const, mode: 'api' as const, status: 'active' as const,
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

  it('uses one canonical source label with agent, title, current state, and a stable masked id suffix', () => {
    const label = sourceLabel({
      source: 'account', id: 'account-1234', agentId: 'claude', title: 'Work OAuth', isCurrent: true,
    });
    expect(label).toBe('Claude Code · Work OAuth · 当前 · …1234');
    expect(label).not.toMatch(/账户|Provider|connectionId/i);
  });

  it('preserves successful resources when another pool or bridge status fails', async () => {
    const profile = {
      id: 'bridge-1', name: 'Bridge', sourceKind: 'provider' as const, sourceId: 'source-9876',
      targetAgentId: 'codex' as const, route: 'local_bridge' as const, mode: 'api' as const, status: 'active' as const,
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

  it('keeps the last successful profiles when a later listProfiles call fails', async () => {
    const profile = {
      id: 'adapter-1', name: 'Kimi → Claude', sourceKind: 'provider' as const, sourceId: 'kimi-1',
      targetAgentId: 'claude' as const, route: 'native_endpoint' as const, mode: 'api' as const,
      status: 'active' as const, ruleId: 'direct', ruleVersion: '1', generatedProviderId: 'generated-1',
      localPort: null, autoStart: false, createdAt: '2026-08-12T00:00:00Z', updatedAt: '2026-08-12T00:00:00Z',
    };
    const previous = await loadAdapterProfileResources({
      listProfiles: async () => [profile],
      getBridgeStatus: async () => ({ profileId: 'unused', state: 'stopped' }),
    });
    const failed = await loadAdapterProfileResources({
      listProfiles: async () => Promise.reject(new Error('profiles unavailable')) as Promise<never[]>,
      getBridgeStatus: async () => ({ profileId: 'unused', state: 'stopped' }),
    });
    const merged = mergeAdapterProfileLoad(previous, failed);

    expect(failed.profiles).toEqual([]);
    expect(merged.profiles).toEqual([profile]);
    expect(merged.profileState).toBe('error');
    expect(merged.profileError).toBeInstanceOf(Error);
  });

  it('commits apply success before deciding whether to probe bridge runtime state', () => {
    const result: Pick<AdapterApplyResult, 'profile'> = {
      profile: {
        id: 'adapter-1', name: 'Direct', sourceKind: 'account', sourceId: 'account-1234', targetAgentId: 'claude',
        route: 'native_endpoint', mode: 'api', status: 'active', ruleId: 'direct', ruleVersion: '1', generatedProviderId: null,
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
      targetAgentId: 'codex' as const, route: 'local_bridge' as const, mode: 'api' as const, status: 'active' as const,
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
