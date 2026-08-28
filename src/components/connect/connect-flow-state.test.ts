import { describe, expect, it, vi } from 'vitest';
import type { Account, Provider } from '@/lib/types';
import type {
  AdapterApplyPlan,
  AdapterApplyResult,
  AdapterProfile,
  AdapterRouteAnalysis,
} from '@/lib/api/adapter';
import type {
  ConnectFlowDeps,
  ConnectFlowEntry,
  ConnectSourceRef,
  PlanEligibility,
  SourceOption,
} from '@/lib/connect-flow/types';
import { planFanoutKey } from '@/lib/connect-flow/types';
import {
  GENERATED_SOURCE_REUSE_MESSAGE,
  REFRESH_FAILED_APPLIED,
  REFRESH_FAILED_SWITCHED,
  RESULT_ACTIVE,
  beginConfirm,
  canBuildSourceOptions,
  canClose,
  canConfirm,
  canEnterPreview,
  canRetry,
  connectFlowEntryKey,
  connectFlowResultMessage,
  createConnectFlowState,
  describePlanPreview,
  eligibilityOf,
  excludeOwnAgentTargets,
  isOfficialCodexOauthAccount,
  keepOwnAgentTarget,
  isBoundPlanStale,
  isConnectFlowEntryStale,
  isGeneratedAdapterSource,
  isIllegalSourceRef,
  isOptionSelectable,
  isPreviewInvalid,
  isProfilesReadyForEntry,
  isTargetSelectable,
  lookupSourceRecord,
  notifyConnectionChangedSafe,
  planEligibilityAllowsApply,
  reduceConnectFlow,
  releaseConfirmLock,
  resolveEmptyKind,
  resolvePreset,
  settleConfirm,
  shouldRevertPreviewToSelect,
  shouldShowPreviewImportHint,
  shouldShowSelectSkeleton,
  sourceAgentIdOf,
  tryAcquireConfirmLock,
  agentsForRouteEndpoint,
  eligibilityForRouteEndpoint,
  representativeAgentForRouteEndpoint,
  visibleTargetsForPurpose,
  type ConnectFlowState,
} from './connect-flow-state';

function account(partial: Partial<Account> & Pick<Account, 'id' | 'agentId'>): Account {
  return {
    kind: 'oauth',
    label: partial.label ?? partial.id,
    isCurrent: false,
    tokenValid: true,
    ...partial,
  };
}

function provider(partial: Partial<Provider> & Pick<Provider, 'id' | 'agentId'>): Provider {
  return {
    name: partial.name ?? partial.id,
    preset: 'custom',
    configText: '{}',
    configFormat: 'json',
    isCurrent: false,
    ...partial,
  };
}

function analysis(overrides: Partial<AdapterRouteAnalysis> = {}): AdapterRouteAnalysis {
  return {
    route: 'native_endpoint',
    support: 'stable',
    reason: '直连端点映射',
    actions: [],
    limitations: [],
    evidence: [],
    ...overrides,
  };
}

function plan(overrides: Partial<AdapterApplyPlan> = {}): AdapterApplyPlan {
  return {
    analysis: analysis(),
    targetAgentId: 'claude',
    canApply: true,
    serviceImpact: 'none',
    changes: [],
    ...overrides,
  };
}

function profile(overrides: Partial<AdapterProfile> = {}): AdapterProfile {
  return {
    id: 'prof-1',
    name: 'route',
    sourceKind: 'provider',
    sourceId: 'prov-kimi',
    targetAgentId: 'claude',
    route: 'native_endpoint',
    mode: 'api',
    status: 'active',
    ruleId: 'kimi-to-claude',
    ruleVersion: '1',
    autoStart: false,
    createdAt: '2026-01-01T00:00:00Z',
    updatedAt: '2026-01-01T00:00:00Z',
    ...overrides,
  };
}

function applyResult(isCurrent: boolean): AdapterApplyResult {
  return {
    profile: profile(),
    provider: provider({ id: 'generated', agentId: 'claude', isCurrent }),
  };
}

function option(partial: Partial<SourceOption> & Pick<SourceOption, 'ref' | 'state'>): SourceOption {
  return {
    group: partial.group ?? (partial.state.kind === 'plannable' ? 'cross' : 'native'),
    agentId: partial.agentId ?? 'claude',
    label: partial.label ?? partial.ref.id,
    ...partial,
  };
}

function readyEligibility(canApply: boolean, extra: Partial<AdapterApplyPlan> = {}): PlanEligibility {
  const next = plan({ canApply, ...extra });
  return {
    kind: 'ready',
    plan: next,
    canApply,
    routeSummary: next.analysis.reason,
    reason: canApply ? undefined : next.analysis.reason,
  };
}

function fakeDeps(overrides: Partial<ConnectFlowDeps> = {}): ConnectFlowDeps {
  return {
    plan: vi.fn(async () => plan()),
    apply: vi.fn(async () => applyResult(true)),
    listProfiles: vi.fn(async () => []),
    switchNative: vi.fn(async () => undefined),
    buildSourceOptions: vi.fn(() => []),
    isOauthIncomplete: vi.fn(() => false),
    createPlanFanout: vi.fn(),
    ...overrides,
  };
}

const forAgent: ConnectFlowEntry = { mode: 'for-agent', targetAgentId: 'claude' };
const kimiSource: ConnectSourceRef = { kind: 'provider', id: 'prov-kimi' };
const forSource: ConnectFlowEntry = { mode: 'for-source', source: kimiSource };

const kimiProvider = provider({ id: 'prov-kimi', agentId: 'kimi', name: 'Kimi Key' });
const claudeAccount = account({ id: 'acc-claude', agentId: 'claude', label: 'me@claude', isCurrent: true });
const claudeSpare = account({ id: 'acc-spare', agentId: 'claude', label: 'spare' });

function selectSwitchable(state: ConnectFlowState, item: SourceOption): ConnectFlowState {
  return reduceConnectFlow(state, { type: 'select_source', option: item });
}

function enterApplyPreview(state: ConnectFlowState, item: SourceOption, eligibility: PlanEligibility): ConnectFlowState {
  const selected = reduceConnectFlow(state, { type: 'select_source', option: item });
  return reduceConnectFlow(selected, { type: 'enter_preview', option: item, eligibility });
}

describe('进入模式 × 预选参数矩阵', () => {
  it('for-agent 固定目标、来源未预选', () => {
    const state = createConnectFlowState(forAgent);
    expect(state.entry).toEqual(forAgent);
    expect(state.selectedTargetAgentId).toBe('claude');
    expect(state.selectedSource).toBeNull();
    expect(state.step).toBe('select');
    expect(resolvePreset(forAgent, [claudeAccount], [])).toEqual({ status: 'ok' });
  });

  it('for-agent 空目标视为非法', () => {
    const entry: ConnectFlowEntry = { mode: 'for-agent', targetAgentId: '   ' };
    const state = createConnectFlowState(entry);
    expect(state.selectedTargetAgentId).toBeNull();
    expect(resolvePreset(entry, [], [])).toEqual({
      status: 'invalid',
      message: '目标工具参数非法',
    });
  });

  it('for-source 固定合法来源、目标未预选', () => {
    const state = createConnectFlowState(forSource);
    expect(state.selectedSource).toEqual(kimiSource);
    expect(state.selectedTargetAgentId).toBeNull();
    expect(resolvePreset(forSource, [], [kimiProvider])).toEqual({ status: 'ok' });
    expect(sourceAgentIdOf(forSource, [], [kimiProvider])).toBe('kimi');
  });

  it('for-source 空 id / 非法 kind 视为非法来源', () => {
    const empty: ConnectFlowEntry = { mode: 'for-source', source: { kind: 'account', id: '' } };
    expect(isIllegalSourceRef(empty.source)).toBe(true);
    expect(createConnectFlowState(empty).selectedSource).toBeNull();
    expect(resolvePreset(empty, [claudeAccount], [])).toEqual({
      status: 'invalid',
      message: '来源参数非法',
    });

    const badKind = { kind: 'token', id: 'x' } as unknown as ConnectSourceRef;
    expect(isIllegalSourceRef(badKind)).toBe(true);
  });

  it('for-source 池中找不到来源视为已删除', () => {
    expect(lookupSourceRecord(kimiSource, [claudeAccount], [])).toBeNull();
    expect(resolvePreset(forSource, [claudeAccount], [])).toEqual({
      status: 'deleted',
      message: '来源已删除',
    });
    expect(resolveEmptyKind({
      poolState: 'ready',
      poolErrors: {},
      accounts: [claudeAccount],
      providers: [],
      options: [],
      eligibilities: new Map(),
      entry: forSource,
      visibleTargetAgentIds: ['claude'],
    })).toEqual({ kind: 'preset_deleted', message: '来源已删除' });
  });
});

describe('for-source 排除自身 Agent', () => {
  it('目标网格去掉来源所属 Agent', () => {
    expect(excludeOwnAgentTargets(['claude', 'kimi', 'codex'], 'kimi')).toEqual(['claude', 'codex']);
  });

  it('keeps Codex for official Codex oauth self-bind', () => {
    expect(isOfficialCodexOauthAccount({ agentId: 'codex', kind: 'oauth' })).toBe(true);
    expect(isOfficialCodexOauthAccount({ agentId: 'codex', kind: 'apikey' })).toBe(false);
    expect(excludeOwnAgentTargets(['claude', 'kimi', 'codex'], 'codex', true))
      .toEqual(['claude', 'kimi', 'codex']);
    expect(excludeOwnAgentTargets(['claude', 'kimi', 'codex'], 'codex'))
      .toEqual(['claude', 'kimi']);
  });

  it('select_target accepts own Codex when allowOwnAgent', () => {
    const state = createConnectFlowState(forSource);
    const allowed = reduceConnectFlow(state, {
      type: 'select_target',
      agentId: 'kimi',
      sourceAgentId: 'kimi',
      allowOwnAgent: true,
    });
    expect(allowed.selectedTargetAgentId).toBe('kimi');
  });

  it('select_target 忽略来源自身 Agent', () => {
    const state = createConnectFlowState(forSource);
    const blocked = reduceConnectFlow(state, {
      type: 'select_target',
      agentId: 'kimi',
      sourceAgentId: 'kimi',
    });
    expect(blocked.selectedTargetAgentId).toBeNull();
    expect(blocked.selectionGeneration).toBe(0);

    const allowed = reduceConnectFlow(state, {
      type: 'select_target',
      agentId: 'claude',
      sourceAgentId: 'kimi',
    });
    expect(allowed.selectedTargetAgentId).toBe('claude');
    expect(allowed.selectionGeneration).toBe(1);
  });

  it('purpose=route keeps the source agent instead of dropping it', () => {
    const grokAccount = account({ id: 'acc-grok', agentId: 'grok', kind: 'oauth' });
    const entry: ConnectFlowEntry = {
      mode: 'for-source',
      source: { kind: 'account', id: 'acc-grok' },
      purpose: 'route',
    };
    expect(keepOwnAgentTarget(entry, [grokAccount])).toBe(true);
    expect(excludeOwnAgentTargets(['claude', 'grok', 'codex'], 'grok', keepOwnAgentTarget(entry, [grokAccount])))
      .toEqual(['claude', 'grok', 'codex']);

    const selected = reduceConnectFlow(createConnectFlowState(entry), {
      type: 'select_target',
      agentId: 'grok',
      sourceAgentId: 'grok',
      allowOwnAgent: keepOwnAgentTarget(entry, [grokAccount]),
    });
    expect(selected.selectedTargetAgentId).toBe('grok');
  });

  it('purpose=share still drops own agent except official Codex oauth', () => {
    const grokAccount = account({ id: 'acc-grok', agentId: 'grok', kind: 'oauth' });
    const share: ConnectFlowEntry = {
      mode: 'for-source',
      source: { kind: 'account', id: 'acc-grok' },
      purpose: 'share',
    };
    expect(keepOwnAgentTarget(share, [grokAccount])).toBe(false);
    const blocked = reduceConnectFlow(createConnectFlowState(share), {
      type: 'select_target',
      agentId: 'grok',
      sourceAgentId: 'grok',
      allowOwnAgent: keepOwnAgentTarget(share, [grokAccount]),
    });
    expect(blocked.selectedTargetAgentId).toBeNull();

    const codexOauth = account({ id: 'acc-codex', agentId: 'codex', kind: 'oauth' });
    const shareCodex: ConnectFlowEntry = {
      mode: 'for-source',
      source: { kind: 'account', id: 'acc-codex' },
      purpose: 'share',
    };
    expect(keepOwnAgentTarget(shareCodex, [codexOauth])).toBe(true);
    const codexKey = account({ id: 'acc-codex-key', agentId: 'codex', kind: 'apikey' });
    expect(keepOwnAgentTarget({
      mode: 'for-source',
      source: { kind: 'account', id: 'acc-codex-key' },
      purpose: 'share',
    }, [codexKey])).toBe(false);
    expect(keepOwnAgentTarget({
      mode: 'for-source',
      source: { kind: 'provider', id: 'prov-codex' },
      purpose: 'share',
    }, [codexOauth])).toBe(false);
  });
});

describe('原生切换分支', () => {
  const current = option({
    ref: { kind: 'account', id: 'acc-claude' },
    state: { kind: 'current' },
    label: '当前账号',
  });
  const switchable = option({
    ref: { kind: 'account', id: 'acc-spare' },
    state: { kind: 'switchable' },
    label: '备用账号',
  });
  const blocked = option({
    ref: { kind: 'account', id: 'acc-blocked' },
    state: { kind: 'blocked_native', reason: '该 Agent 不支持账号切换' },
    label: '不可切换',
  });

  it('current 禁用，不可选、不可进预览', () => {
    const state = createConnectFlowState(forAgent);
    expect(isOptionSelectable(current)).toBe(false);
    const next = reduceConnectFlow(state, { type: 'select_source', option: current });
    expect(next.selectedSource).toBeNull();
    expect(canEnterPreview(next, current)).toBe(false);
  });

  it('blocked_native 置灰保留原因，不可选', () => {
    expect(isOptionSelectable(blocked)).toBe(false);
    expect(blocked.state.kind === 'blocked_native' && blocked.state.reason).toBe('该 Agent 不支持账号切换');
    const state = reduceConnectFlow(createConnectFlowState(forAgent), {
      type: 'select_source',
      option: blocked,
    });
    expect(state.selectedSource).toBeNull();
  });

  it('switchable 走预览并调用 switchNative', async () => {
    const deps = fakeDeps();
    let state = selectSwitchable(createConnectFlowState(forAgent), switchable);
    expect(canEnterPreview(state, switchable)).toBe(true);
    state = reduceConnectFlow(state, { type: 'enter_preview', option: switchable });
    expect(state.step).toBe('preview');
    expect(state.previewKind).toBe('switch');
    expect(canConfirm(state)).toBe(true);

    const begun = beginConfirm(state);
    expect(begun.allowed).toBe(true);
    expect(begun.next.busy).toBe('switching');
    expect(canClose(begun.next)).toBe(false);

    const settled = await settleConfirm({
      state: begun.next,
      startedGeneration: begun.next.selectionGeneration,
      deps,
      option: switchable,
    });
    expect(settled.called).toBe('switch');
    expect(deps.switchNative).toHaveBeenCalledTimes(1);
    expect(deps.apply).not.toHaveBeenCalled();

    state = reduceConnectFlow(begun.next, settled.event);
    expect(state.step).toBe('result');
    expect(state.result?.kind).toBe('switched');
    expect(connectFlowResultMessage(state.result!)).toBe('已切换');
  });
});

describe('busy 锁', () => {
  const switchable = option({
    ref: { kind: 'account', id: 'acc-spare' },
    state: { kind: 'switchable' },
  });
  const plannable = option({
    ref: kimiSource,
    state: { kind: 'plannable' },
    agentId: 'kimi',
    group: 'cross',
  });

  it('apply 进行中禁止重复提交与关闭、忽略选择变更', () => {
    let state = enterApplyPreview(
      createConnectFlowState(forAgent),
      plannable,
      readyEligibility(true),
    );
    state = reduceConnectFlow(state, { type: 'begin_apply' });
    expect(state.busy).toBe('applying');
    expect(canClose(state)).toBe(false);
    expect(canConfirm(state)).toBe(false);
    expect(beginConfirm(state).allowed).toBe(false);

    const ignored = reduceConnectFlow(state, { type: 'select_source', option: switchable });
    expect(ignored.selectedSource).toEqual(kimiSource);
    expect(ignored.selectionGeneration).toBe(state.selectionGeneration);
    expect(reduceConnectFlow(state, { type: 'back_to_select' })).toBe(state);
    expect(reduceConnectFlow(state, { type: 'begin_apply' }).busy).toBe('applying');
  });

  it('switch 进行中同样锁关闭与重入', () => {
    let state = reduceConnectFlow(
      selectSwitchable(createConnectFlowState(forAgent), switchable),
      { type: 'enter_preview', option: switchable },
    );
    state = reduceConnectFlow(state, { type: 'begin_switch' });
    expect(canClose(state)).toBe(false);
    expect(beginConfirm(state).allowed).toBe(false);
    expect(reduceConnectFlow(state, { type: 'enter_preview', option: switchable })).toBe(state);
  });
});

describe('stale plan 不得 apply', () => {
  const first = option({
    ref: kimiSource,
    state: { kind: 'plannable' },
    agentId: 'kimi',
    group: 'cross',
  });
  const second = option({
    ref: { kind: 'provider', id: 'prov-other' },
    state: { kind: 'plannable' },
    agentId: 'codex',
    group: 'cross',
  });

  it('选择切换后旧 plan 作废，settleConfirm 不调用 apply', async () => {
    const eligibility = readyEligibility(true);
    let state = enterApplyPreview(createConnectFlowState(forAgent), first, eligibility);
    expect(state.boundPlan?.generation).toBe(1);
    expect(isBoundPlanStale(state)).toBe(false);

    state = reduceConnectFlow(state, { type: 'select_source', option: second });
    expect(state.step).toBe('select');
    expect(state.boundPlan).toBeNull();
    expect(state.selectionGeneration).toBe(2);
    expect(isBoundPlanStale(state)).toBe(true);
    expect(canConfirm(state)).toBe(false);
    expect(beginConfirm(state).allowed).toBe(false);

    const staleBusy: ConnectFlowState = {
      ...state,
      step: 'preview',
      previewKind: 'apply',
      busy: 'applying',
      boundPlan: {
        generation: 1,
        source: first.ref,
        targetAgentId: 'claude',
        plan: plan({ canApply: true }),
      },
    };
    const deps = fakeDeps();
    const settled = await settleConfirm({
      state: staleBusy,
      startedGeneration: 1,
      deps,
      option: first,
    });
    expect(settled.called).toBeNull();
    expect(settled.event.type).toBe('apply_rejected_stale');
    expect(deps.apply).not.toHaveBeenCalled();
  });

  it('ready 但 canApply=false 不能进预览、不能确认', () => {
    const blocked = readyEligibility(false, { analysis: analysis({ reason: '能力矩阵关闭' }) });
    const state = selectSwitchable(createConnectFlowState(forAgent), first);
    expect(canEnterPreview(state, first, blocked)).toBe(false);
    const preview = reduceConnectFlow(state, { type: 'enter_preview', option: first, eligibility: blocked });
    expect(preview.step).toBe('select');
    expect(preview.boundPlan).toBeNull();
  });
});

describe('成功 / 失败 / 刷新失败三态', () => {
  const plannable = option({
    ref: kimiSource,
    state: { kind: 'plannable' },
    agentId: 'kimi',
    group: 'cross',
  });

  it('apply 成功以 provider.isCurrent 为权威显示已生效', async () => {
    const deps = fakeDeps({ apply: vi.fn(async () => applyResult(true)) });
    let state = enterApplyPreview(createConnectFlowState(forAgent), plannable, readyEligibility(true));
    const begun = beginConfirm(state);
    const settled = await settleConfirm({
      state: begun.next,
      startedGeneration: begun.next.selectionGeneration,
      deps,
      option: plannable,
    });
    state = reduceConnectFlow(begun.next, settled.event);
    expect(state.result).toMatchObject({ kind: 'applied', isCurrent: true, refreshFailed: false });
    expect(connectFlowResultMessage(state.result!)).toBe(RESULT_ACTIVE);
  });

  it('apply 成功但 isCurrent=false 不显示已生效', () => {
    let state = enterApplyPreview(createConnectFlowState(forAgent), plannable, readyEligibility(true));
    state = reduceConnectFlow(state, { type: 'begin_apply' });
    state = reduceConnectFlow(state, {
      type: 'apply_succeeded',
      generation: state.selectionGeneration,
      result: applyResult(false),
    });
    expect(state.result).toMatchObject({ kind: 'applied', isCurrent: false });
    expect(connectFlowResultMessage(state.result!)).toBe('已应用');
  });

  it('apply 失败保留选择与预览，可重试', async () => {
    const deps = fakeDeps({
      apply: vi.fn(async () => {
        throw new Error('上游拒绝写入');
      }),
    });
    let state = enterApplyPreview(createConnectFlowState(forAgent), plannable, readyEligibility(true));
    const generation = state.selectionGeneration;
    const bound = state.boundPlan;
    const begun = beginConfirm(state);
    const settled = await settleConfirm({
      state: begun.next,
      startedGeneration: generation,
      deps,
      option: plannable,
    });
    state = reduceConnectFlow(begun.next, settled.event);
    expect(state.step).toBe('result');
    expect(state.result).toEqual({ kind: 'failed', action: 'apply', error: '上游拒绝写入' });
    expect(state.selectedSource).toEqual(kimiSource);
    expect(state.boundPlan).toEqual(bound);
    expect(canRetry(state)).toBe(true);

    state = reduceConnectFlow(state, { type: 'retry_from_result' });
    expect(state.step).toBe('preview');
    expect(state.boundPlan).toEqual(bound);
    expect(state.result).toBeNull();
    expect(canConfirm(state)).toBe(true);
  });

  it('apply 成功后刷新失败显示已应用但列表刷新失败', async () => {
    let state = enterApplyPreview(createConnectFlowState(forAgent), plannable, readyEligibility(true));
    state = reduceConnectFlow(state, { type: 'begin_apply' });
    state = reduceConnectFlow(state, {
      type: 'apply_succeeded',
      generation: state.selectionGeneration,
      result: applyResult(true),
    });
    const refreshed = await notifyConnectionChangedSafe(
      { kind: 'applied', result: applyResult(true) },
      async () => {
        throw new Error('reload failed');
      },
    );
    expect(refreshed).toBe(false);
    state = reduceConnectFlow(state, { type: 'refresh_failed' });
    expect(state.result).toMatchObject({ kind: 'applied', isCurrent: true, refreshFailed: true });
    expect(connectFlowResultMessage(state.result!)).toBe(REFRESH_FAILED_APPLIED);
  });

  it('原生切换成功后刷新失败显示已切换但列表刷新失败', async () => {
    const switchable = option({
      ref: { kind: 'account', id: 'acc-spare' },
      state: { kind: 'switchable' },
    });
    let state = reduceConnectFlow(
      selectSwitchable(createConnectFlowState(forAgent), switchable),
      { type: 'enter_preview', option: switchable },
    );
    state = reduceConnectFlow(state, { type: 'begin_switch' });
    state = reduceConnectFlow(state, {
      type: 'switch_succeeded',
      ref: switchable.ref,
      agentId: 'claude',
    });
    const refreshed = await notifyConnectionChangedSafe(
      { kind: 'switched', ref: switchable.ref, agentId: 'claude' },
      () => {
        throw new Error('reload failed');
      },
    );
    expect(refreshed).toBe(false);
    state = reduceConnectFlow(state, { type: 'refresh_failed' });
    expect(connectFlowResultMessage(state.result!)).toBe(REFRESH_FAILED_SWITCHED);
  });
});

describe('空态', () => {
  it('钱包为空（池已就绪且无凭据）', () => {
    expect(resolveEmptyKind({
      poolState: 'ready',
      poolErrors: {},
      accounts: [],
      providers: [],
      options: [],
      eligibilities: new Map(),
      entry: forAgent,
    })).toEqual({ kind: 'wallet_empty' });
  });

  it('有当前登录时不判全部不可行', () => {
    const options: SourceOption[] = [
      option({
        ref: { kind: 'account', id: 'acc-claude' },
        state: { kind: 'current' },
      }),
      option({
        ref: { kind: 'account', id: 'acc-blocked' },
        state: { kind: 'blocked_native', reason: '不可切换' },
      }),
      option({
        ref: kimiSource,
        state: { kind: 'plannable' },
        agentId: 'kimi',
        group: 'cross',
      }),
    ];
    const eligibilities = new Map<string, PlanEligibility>([
      [planFanoutKey({ source: kimiSource, targetAgentId: 'claude' }), readyEligibility(false)],
    ]);
    expect(resolveEmptyKind({
      poolState: 'ready',
      poolErrors: {},
      accounts: [claudeAccount, claudeSpare],
      providers: [kimiProvider],
      options,
      eligibilities,
      entry: forAgent,
    })).toEqual({ kind: 'none' });
  });

  it('无当前登录且全部不可行', () => {
    const options: SourceOption[] = [
      option({
        ref: { kind: 'account', id: 'acc-blocked' },
        state: { kind: 'blocked_native', reason: '不可切换' },
      }),
      option({
        ref: kimiSource,
        state: { kind: 'plannable' },
        agentId: 'kimi',
        group: 'cross',
      }),
    ];
    const eligibilities = new Map<string, PlanEligibility>([
      [planFanoutKey({ source: kimiSource, targetAgentId: 'claude' }), readyEligibility(false)],
    ]);
    expect(resolveEmptyKind({
      poolState: 'ready',
      poolErrors: {},
      accounts: [claudeSpare],
      providers: [kimiProvider],
      options,
      eligibilities,
      entry: forAgent,
    })).toEqual({ kind: 'all_infeasible' });
  });

  it('资源部分加载失败不得当空池', () => {
    const empty = resolveEmptyKind({
      poolState: 'partial',
      poolErrors: { providers: new Error('providers down') },
      accounts: [],
      providers: [],
      options: [],
      eligibilities: new Map(),
      entry: forAgent,
    });
    expect(empty.kind).toBe('partial_load_error');
    if (empty.kind === 'partial_load_error') {
      expect(empty.message).toContain('供应商');
    }

    const profilesFailed = resolveEmptyKind({
      poolState: 'ready',
      poolErrors: {},
      profilesError: new Error('profiles down'),
      accounts: [],
      providers: [],
      options: [],
      eligibilities: new Map(),
      entry: forAgent,
    });
    expect(profilesFailed.kind).toBe('partial_load_error');
  });

  it('可行性仍在 loading 时不判全部不可行', () => {
    const options: SourceOption[] = [
      option({
        ref: kimiSource,
        state: { kind: 'plannable' },
        agentId: 'kimi',
        group: 'cross',
      }),
    ];
    expect(resolveEmptyKind({
      poolState: 'ready',
      poolErrors: {},
      accounts: [],
      providers: [kimiProvider],
      options,
      eligibilities: new Map([
        [planFanoutKey({ source: kimiSource, targetAgentId: 'claude' }), { kind: 'loading' }],
      ]),
      entry: forAgent,
    })).toEqual({ kind: 'none' });
  });
});

describe('plan 预览人话化', () => {
  const forbiddenPreviewCopy = [
    '请保持 AgentHub',
    '只连这台电脑',
    '不会进 Claude',
    '过期',
    '会经本机路由',
    'ANTHROPIC_',
    'Messages',
    '将写入的配置',
    '可应用',
    '③ 本机协议桥',
    '127.0.0.1',
  ];

  function previewText(view: ReturnType<typeof describePlanPreview>): string {
    return [view.title, view.reason, ...view.notes].join('\n');
  }

  it('Grok→Claude local_bridge 只保留三秒可读文案', () => {
    const view = describePlanPreview(plan({
      analysis: analysis({
        route: 'local_bridge',
        support: 'experimental',
        reason: 'Grok 登录会经本机路由接到 Claude Code。',
        limitations: [
          '会把 Claude 的 ANTHROPIC_BASE_URL / ANTHROPIC_AUTH_TOKEN 指向本机 loopback；上游 xAI OAuth token 不进 Claude。',
          '实验性协议桥接：Claude Messages → xAI Responses (cli-chat-proxy)；AgentHub 需保持在托盘运行。',
        ],
      }),
      reusePath: 'local_bridge',
      serviceImpact: 'requires_local_bridge',
      changes: [
        { target: 'claude', field: 'ANTHROPIC_BASE_URL', value: 'http://127.0.0.1:<本机端口>', secret: false },
        { target: 'claude', field: 'ANTHROPIC_AUTH_TOKEN', secret: true },
      ],
    }));
    expect(view.title).toBe('本机路由');
    expect(view.experimental).toBe(true);
    expect(view.reason).toBe('用这份 Grok 登录接到 /v1/messages。');
    expect(view.notes).toEqual(['关掉会进托盘，路由继续跑。']);
    const text = previewText(view);
    for (const banned of forbiddenPreviewCopy) {
      expect(text).not.toContain(banned);
    }
  });

  it('Codex 官方登录接到 Grok / Kimi / DSH 预览标题是本机路由', () => {
    for (const [agentId, path] of [
      ['grok', '/v1/responses'],
      ['kimi', '/v1/chat/completions'],
      ['dsh', '/v1/chat/completions'],
    ] as const) {
      const view = describePlanPreview(plan({
        targetAgentId: agentId,
        analysis: analysis({
          route: 'local_bridge',
          support: 'experimental',
          reason: `Codex 官方登录会经本机路由接到 ${agentId}。`,
        }),
        reusePath: 'local_bridge',
        serviceImpact: 'requires_local_bridge',
      }));
      expect(view.title).toBe('本机路由');
      expect(view.reason).toBe(`用这份 Codex / ChatGPT 登录接到 ${path}。`);
      expect(view.reason).not.toContain('实验');
      expect(view.reason).not.toContain('未验证');
    }
  });

  it('Grok→Codex local_bridge 预览接到 Responses 端点而不是 Agent 名', () => {
    const view = describePlanPreview(plan({
      targetAgentId: 'codex',
      analysis: analysis({
        route: 'local_bridge',
        support: 'experimental',
        reason: 'Grok 登录会经本机路由接到 Codex。',
      }),
      reusePath: 'local_bridge',
      serviceImpact: 'requires_local_bridge',
    }));
    expect(view.title).toBe('本机路由');
    expect(view.reason).toBe('用这份 Grok 登录接到 /v1/responses。');
    expect(view.reason).not.toMatch(/接到 codex/);
  });

  it('renders native subscription reuse as a single human note', () => {
    const view = describePlanPreview(plan({
      analysis: analysis({
        route: 'config_sync',
        gateKind: 'preview_only',
        reason: '原生订阅预览',
      }),
      canApply: false,
      reusePath: 'native_subscription',
      serviceImpact: 'none',
      changes: [
        { target: 'pi', field: 'provider', value: 'openai-codex', secret: false },
        { target: 'pi', field: 'apiKey', secret: true },
      ],
    }));
    expect(view.title).toBe('用这份登录');
    expect(view.notes).toHaveLength(1);
    const text = previewText(view);
    expect(text).not.toMatch(/桥|写入|ANTHROPIC_|本机协议|loopback/i);
  });
});

describe('eligibility 查找含 kind 防碰撞', () => {
  it('account 与 provider 同 id 不串 key', () => {
    const accountRef: ConnectSourceRef = { kind: 'account', id: 'same' };
    const providerRef: ConnectSourceRef = { kind: 'provider', id: 'same' };
    const map = new Map<string, PlanEligibility>([
      [planFanoutKey({ source: accountRef, targetAgentId: 'claude' }), readyEligibility(true)],
    ]);
    expect(eligibilityOf(map, accountRef, 'claude')?.kind).toBe('ready');
    expect(eligibilityOf(map, providerRef, 'claude')).toBeUndefined();
  });
});

describe('for-source 预览与 apply', () => {
  it('下一步 canEnterPreview 与 enter_preview 同门禁（含 Codex 本机路由目标）', () => {
    const codexSource: ConnectFlowEntry = {
      mode: 'for-source',
      source: { kind: 'account', id: 'codex-live-1' },
    };
    const targets = ['claude', 'grok', 'kimi', 'dsh'] as const;
    for (const agentId of targets) {
      let state = createConnectFlowState(codexSource);
      state = reduceConnectFlow(state, {
        type: 'select_target',
        agentId,
        sourceAgentId: 'codex',
        allowOwnAgent: true,
      });
      const localPlan = plan({
        canApply: true,
        targetAgentId: agentId,
        analysis: analysis({
          route: 'local_bridge',
          support: 'experimental',
          reason: 'Codex 官方登录会经本机路由接到目标。',
        }),
        reusePath: 'local_bridge',
      });
      const ready: PlanEligibility = {
        kind: 'ready',
        plan: localPlan,
        canApply: true,
        routeSummary: '本机路由',
      };
      const displayTruePlanFalse: PlanEligibility = {
        kind: 'ready',
        plan: { ...localPlan, canApply: false },
        canApply: true,
        routeSummary: '本机路由',
      };
      const cases: Array<{ eligibility: PlanEligibility | undefined; wantPreview: boolean }> = [
        { eligibility: ready, wantPreview: true },
        { eligibility: displayTruePlanFalse, wantPreview: false },
        { eligibility: { kind: 'loading' }, wantPreview: false },
        { eligibility: { kind: 'error', message: 'fail' }, wantPreview: false },
        { eligibility: undefined, wantPreview: false },
      ];
      for (const item of cases) {
        expect(canEnterPreview(state, null, item.eligibility)).toBe(item.wantPreview);
        expect(isTargetSelectable(item.eligibility)).toBe(item.wantPreview);
        const next = reduceConnectFlow(state, {
          type: 'enter_preview',
          eligibility: item.eligibility,
        });
        expect(next.step).toBe(item.wantPreview ? 'preview' : 'select');
        if (item.wantPreview) {
          expect(next.previewKind).toBe('apply');
          expect(next.boundPlan?.targetAgentId).toBe(agentId);
          expect(next.boundPlan?.plan.canApply).toBe(true);
        } else {
          expect(next).toBe(state);
        }
      }
    }
  });

  it('选中可行目标后绑定 plan 并 apply', async () => {
    const eligibility = readyEligibility(true, { targetAgentId: 'claude' });
    let state = createConnectFlowState(forSource);
    state = reduceConnectFlow(state, {
      type: 'select_target',
      agentId: 'claude',
      sourceAgentId: 'kimi',
    });
    expect(canEnterPreview(state, null, eligibility)).toBe(true);
    state = reduceConnectFlow(state, { type: 'enter_preview', eligibility });
    expect(state.previewKind).toBe('apply');
    expect(state.boundPlan?.targetAgentId).toBe('claude');
    expect(state.boundPlan?.source).toEqual(kimiSource);

    const deps = fakeDeps();
    const begun = beginConfirm(state);
    const settled = await settleConfirm({
      state: begun.next,
      startedGeneration: begun.next.selectionGeneration,
      deps,
      option: null,
    });
    expect(settled.called).toBe('apply');
    expect(deps.apply).toHaveBeenCalledWith({
      sourceKind: 'provider',
      sourceId: 'prov-kimi',
      targetAgentId: 'claude',
    });
  });
});

describe('profiles 未就绪与 generated 来源', () => {
  it('loadedKey 与当前 entry 不一致视为未就绪', () => {
    expect(isProfilesReadyForEntry(null, 'for-agent:claude')).toBe(false);
    expect(isProfilesReadyForEntry('for-agent:kimi', 'for-agent:claude')).toBe(false);
    expect(isProfilesReadyForEntry('for-agent:claude', 'for-agent:claude')).toBe(true);
    expect(canBuildSourceOptions(false)).toBe(false);
    expect(canBuildSourceOptions(true, new Error('profiles down'))).toBe(false);
    expect(canBuildSourceOptions(true)).toBe(true);
  });

  it('profiles 未就绪时显示骨架，即使已有 options 或目标网格', () => {
    expect(shouldShowSelectSkeleton({
      profilesReady: false,
      poolLoading: false,
      optionsLength: 3,
      targetAgentIdsLength: 2,
    })).toBe(true);
    expect(shouldShowSelectSkeleton({
      profilesReady: true,
      poolLoading: true,
      optionsLength: 0,
      targetAgentIdsLength: 0,
    })).toBe(true);
    expect(shouldShowSelectSkeleton({
      profilesReady: true,
      poolLoading: false,
      optionsLength: 0,
      targetAgentIdsLength: 0,
    })).toBe(false);
  });

  it('for-source 来源命中 generatedProviderId 视为不可再复用', () => {
    const generated = provider({ id: 'gen-claude', agentId: 'claude', name: '兼容路由' });
    const source: ConnectSourceRef = { kind: 'provider', id: 'gen-claude' };
    const entry: ConnectFlowEntry = { mode: 'for-source', source };
    expect(isGeneratedAdapterSource(source, [profile({ generatedProviderId: 'gen-claude' })])).toBe(true);
    expect(isGeneratedAdapterSource({ kind: 'account', id: 'gen-claude' }, [
      profile({ generatedProviderId: 'gen-claude' }),
    ])).toBe(false);
    expect(isGeneratedAdapterSource(source, [profile({ generatedProviderId: null })])).toBe(false);

    expect(resolveEmptyKind({
      poolState: 'ready',
      poolErrors: {},
      profilesReady: true,
      profiles: [profile({ generatedProviderId: 'gen-claude' })],
      accounts: [claudeAccount],
      providers: [generated],
      options: [],
      eligibilities: new Map(),
      entry,
      visibleTargetAgentIds: ['kimi'],
    })).toEqual({ kind: 'preset_invalid', message: GENERATED_SOURCE_REUSE_MESSAGE });

    expect(resolveEmptyKind({
      poolState: 'ready',
      poolErrors: {},
      profilesReady: false,
      profiles: [profile({ generatedProviderId: 'gen-claude' })],
      accounts: [claudeAccount],
      providers: [generated],
      options: [],
      eligibilities: new Map(),
      entry,
      visibleTargetAgentIds: ['kimi'],
    }).kind).not.toBe('preset_invalid');
  });
});

describe('选中项失效退回 select', () => {
  const plannable = option({
    ref: kimiSource,
    state: { kind: 'plannable' },
    agentId: 'kimi',
    group: 'cross',
  });

  it('for-agent 预览步 options 不再包含选中项则应退回', () => {
    expect(shouldRevertPreviewToSelect({
      step: 'preview',
      mode: 'for-agent',
      selectedSource: kimiSource,
      options: [],
    })).toBe(true);
    expect(shouldRevertPreviewToSelect({
      step: 'preview',
      mode: 'for-agent',
      selectedSource: kimiSource,
      options: [plannable],
    })).toBe(false);
    expect(shouldRevertPreviewToSelect({
      step: 'select',
      mode: 'for-agent',
      selectedSource: kimiSource,
      options: [],
    })).toBe(false);
    expect(shouldRevertPreviewToSelect({
      step: 'preview',
      mode: 'for-source',
      selectedSource: kimiSource,
      options: [],
    })).toBe(false);
  });

  it('back_to_select 清除 boundPlan 与 previewKind', () => {
    let state = enterApplyPreview(createConnectFlowState(forAgent), plannable, readyEligibility(true));
    expect(state.boundPlan).not.toBeNull();
    expect(state.previewKind).toBe('apply');
    state = reduceConnectFlow(state, { type: 'back_to_select' });
    expect(state.step).toBe('select');
    expect(state.boundPlan).toBeNull();
    expect(state.previewKind).toBeNull();
  });
});

describe('首帧 entry 不同步', () => {
  it('state.entry 与当前 entry 的 key 不同则视为过期', () => {
    expect(connectFlowEntryKey(forAgent)).toBe('for-agent:claude');
    expect(connectFlowEntryKey(forSource)).toBe('for-source:provider:prov-kimi:all');
    expect(connectFlowEntryKey({ ...forSource, purpose: 'share' })).toBe(
      'for-source:provider:prov-kimi:share',
    );
    expect(connectFlowEntryKey({ ...forSource, purpose: 'route' })).toBe(
      'for-source:provider:prov-kimi:route',
    );
    expect(isConnectFlowEntryStale(forAgent, { mode: 'for-agent', targetAgentId: 'kimi' })).toBe(true);
    expect(isConnectFlowEntryStale(forAgent, forAgent)).toBe(false);
    expect(isConnectFlowEntryStale(forAgent, null)).toBe(true);
    expect(isConnectFlowEntryStale(forAgent, forSource)).toBe(true);
  });
});

describe('可行性门禁以 plan.canApply 为准', () => {
  const plannable = option({
    ref: kimiSource,
    state: { kind: 'plannable' },
    agentId: 'kimi',
    group: 'cross',
  });

  it('两字段矛盾时以 plan.canApply 为准', () => {
    const displayTruePlanFalse: PlanEligibility = {
      kind: 'ready',
      plan: plan({ canApply: false }),
      canApply: true,
      routeSummary: '展示可应用',
    };
    expect(planEligibilityAllowsApply(displayTruePlanFalse)).toBe(false);
    expect(isOptionSelectable(plannable, displayTruePlanFalse)).toBe(false);
    expect(isTargetSelectable(displayTruePlanFalse)).toBe(false);
    const selected = selectSwitchable(createConnectFlowState(forAgent), plannable);
    expect(canEnterPreview(selected, plannable, displayTruePlanFalse)).toBe(false);
    expect(reduceConnectFlow(selected, {
      type: 'enter_preview',
      option: plannable,
      eligibility: displayTruePlanFalse,
    }).step).toBe('select');

    const displayFalsePlanTrue: PlanEligibility = {
      kind: 'ready',
      plan: plan({ canApply: true }),
      canApply: false,
      routeSummary: '展示不可用',
    };
    expect(planEligibilityAllowsApply(displayFalsePlanTrue)).toBe(true);
    expect(isOptionSelectable(plannable, displayFalsePlanTrue)).toBe(true);
    expect(isTargetSelectable(displayFalsePlanTrue)).toBe(true);
    expect(canEnterPreview(selected, plannable, displayFalsePlanTrue)).toBe(true);
    expect(reduceConnectFlow(selected, {
      type: 'enter_preview',
      option: plannable,
      eligibility: displayFalsePlanTrue,
    }).step).toBe('preview');
  });
});

describe('preview 同步失效与确认占锁', () => {
  const plannable = option({
    ref: kimiSource,
    state: { kind: 'plannable' },
    agentId: 'kimi',
    group: 'cross',
  });

  it('for-agent：options 中找不到选中项则 preview invalid（apply 与 switch）', () => {
    const applyState = enterApplyPreview(createConnectFlowState(forAgent), plannable, readyEligibility(true));
    expect(isPreviewInvalid({
      state: applyState,
      options: [plannable],
      accounts: [claudeAccount],
      providers: [kimiProvider],
    })).toBe(false);
    expect(isPreviewInvalid({
      state: applyState,
      options: [],
      accounts: [claudeAccount],
      providers: [kimiProvider],
    })).toBe(true);

    const switchable = option({
      ref: { kind: 'account', id: 'acc-spare' },
      state: { kind: 'switchable' },
    });
    const switchState = reduceConnectFlow(
      selectSwitchable(createConnectFlowState(forAgent), switchable),
      { type: 'enter_preview', option: switchable },
    );
    expect(isPreviewInvalid({
      state: switchState,
      options: [switchable],
      accounts: [claudeSpare],
      providers: [],
    })).toBe(false);
    expect(isPreviewInvalid({
      state: switchState,
      options: [],
      accounts: [claudeSpare],
      providers: [],
    })).toBe(true);
    expect(isPreviewInvalid({
      state: createConnectFlowState(forAgent),
      options: [],
      accounts: [],
      providers: [],
    })).toBe(false);
  });

  it('for-source：pool 中找不到固定来源则 preview invalid', () => {
    let state = createConnectFlowState(forSource);
    state = reduceConnectFlow(state, {
      type: 'select_target',
      agentId: 'claude',
      sourceAgentId: 'kimi',
    });
    state = reduceConnectFlow(state, { type: 'enter_preview', eligibility: readyEligibility(true) });
    expect(isPreviewInvalid({
      state,
      options: [],
      accounts: [claudeAccount],
      providers: [kimiProvider],
    })).toBe(false);
    expect(isPreviewInvalid({
      state,
      options: [],
      accounts: [claudeAccount],
      providers: [],
    })).toBe(true);
  });

  it('for-source 已导入且已登录的 live ticket 不显示导入提示', () => {
    expect(shouldShowPreviewImportHint({
      entry: { mode: 'for-source', source: { kind: 'account', id: 'acc-grok' } },
      option: null,
      accounts: [account({ id: 'acc-grok', agentId: 'grok', tokenValid: true })],
      providers: [],
    })).toBe(false);
  });

  it('来源未导入或未登录时显示导入提示', () => {
    expect(shouldShowPreviewImportHint({
      entry: { mode: 'for-source', source: { kind: 'account', id: 'missing' } },
      option: null,
      accounts: [],
      providers: [],
    })).toBe(true);
    expect(shouldShowPreviewImportHint({
      entry: { mode: 'for-source', source: { kind: 'account', id: 'acc-grok' } },
      option: null,
      accounts: [account({ id: 'acc-grok', agentId: 'grok', tokenValid: false })],
      providers: [],
    })).toBe(true);
  });

  it('for-agent 已导入且已登录的来源不显示导入提示', () => {
    const live = option({
      ref: { kind: 'account', id: 'acc-grok' },
      state: { kind: 'plannable' },
      agentId: 'grok',
      group: 'cross',
      account: account({ id: 'acc-grok', agentId: 'grok', tokenValid: true }),
    });
    expect(shouldShowPreviewImportHint({
      entry: forAgent,
      option: live,
    })).toBe(false);
  });

  it('确认占锁：已持有则再次获取失败，释放后可再获取', () => {
    const lock = { current: false };
    expect(tryAcquireConfirmLock(lock)).toBe(true);
    expect(lock.current).toBe(true);
    expect(tryAcquireConfirmLock(lock)).toBe(false);
    releaseConfirmLock(lock);
    expect(lock.current).toBe(false);
    expect(tryAcquireConfirmLock(lock)).toBe(true);
  });
});

describe('purpose-gated preview', () => {
  it('for-source share cannot preview a local_bridge plan', () => {
    const entry: ConnectFlowEntry = {
      mode: 'for-source',
      source: kimiSource,
      purpose: 'share',
    };
    let state = createConnectFlowState(entry);
    state = reduceConnectFlow(state, {
      type: 'select_target',
      agentId: 'codex',
      sourceAgentId: 'kimi',
    });
    const elig = readyEligibility(true, {
      analysis: analysis({ route: 'local_bridge' }),
    });
    expect(canEnterPreview(state, null, elig)).toBe(false);
  });

  it('for-source route can preview a local_bridge plan', () => {
    const entry: ConnectFlowEntry = {
      mode: 'for-source',
      source: kimiSource,
      purpose: 'route',
    };
    let state = createConnectFlowState(entry);
    state = reduceConnectFlow(state, {
      type: 'select_target',
      agentId: 'codex',
      sourceAgentId: 'kimi',
    });
    const elig = readyEligibility(true, {
      analysis: analysis({ route: 'local_bridge' }),
    });
    expect(canEnterPreview(state, null, elig)).toBe(true);
  });
});

describe('visibleTargetsForPurpose', () => {
  it('keeps loading rows and drops ready plans for the other purpose', () => {
    const map = new Map<string, PlanEligibility>([
      [planFanoutKey({ source: kimiSource, targetAgentId: 'claude' }), readyEligibility(true, {
        analysis: analysis({ route: 'config_sync' }),
      })],
      [planFanoutKey({ source: kimiSource, targetAgentId: 'codex' }), readyEligibility(true, {
        analysis: analysis({ route: 'local_bridge' }),
      })],
      [planFanoutKey({ source: kimiSource, targetAgentId: 'grok' }), { kind: 'loading' }],
    ]);
    expect(visibleTargetsForPurpose(['claude', 'codex', 'grok'], kimiSource, map, 'share')).toEqual([
      'claude',
      'grok',
    ]);
    expect(visibleTargetsForPurpose(['claude', 'codex', 'grok'], kimiSource, map, 'route')).toEqual([
      'codex',
      'grok',
    ]);
  });

  it('share keeps direct/config-sync targets and leaves local_bridge to 本机转发', () => {
    const source = { kind: 'provider' as const, id: 'or-openai' };
    const map = new Map<string, PlanEligibility>([
      [planFanoutKey({ source, targetAgentId: 'pi' }), readyEligibility(true, {
        analysis: analysis({ route: 'config_sync' }),
      })],
      [planFanoutKey({ source, targetAgentId: 'claude' }), readyEligibility(true, {
        analysis: analysis({ route: 'local_bridge', ruleId: 'openai-api-to-claude-v1' }),
      })],
      [planFanoutKey({ source, targetAgentId: 'grok' }), readyEligibility(true, {
        analysis: analysis({ route: 'local_bridge', ruleId: 'openai-api-to-grok-bridge-v1' }),
      })],
      [planFanoutKey({ source, targetAgentId: 'kimi' }), readyEligibility(false, {
        analysis: analysis({ route: 'unsupported' }),
      })],
    ]);
    expect(visibleTargetsForPurpose(['pi', 'claude', 'grok', 'kimi'], source, map, 'share')).toEqual([
      'pi',
      'kimi',
    ]);
    expect(visibleTargetsForPurpose(['pi', 'claude', 'grok', 'kimi'], source, map, 'route')).toEqual([
      'claude',
      'grok',
      'kimi',
    ]);
  });
});

describe('route endpoint grouping', () => {
  it('groups local-bridge writers onto the three unified surfaces', () => {
    const map = new Map<string, PlanEligibility>([
      [planFanoutKey({ source: kimiSource, targetAgentId: 'claude' }), readyEligibility(true, {
        analysis: analysis({ route: 'local_bridge', ruleId: 'kimi-membership-to-claude-v1' }),
      })],
      [planFanoutKey({ source: kimiSource, targetAgentId: 'codex' }), readyEligibility(true, {
        analysis: analysis({ route: 'local_bridge', ruleId: 'kimi-membership-to-codex-v1' }),
      })],
      [planFanoutKey({ source: kimiSource, targetAgentId: 'kimi' }), readyEligibility(false, {
        analysis: analysis({ route: 'local_bridge', ruleId: 'kimi-membership-to-kimi-v1' }),
      })],
    ]);
    const targets = ['claude', 'codex', 'kimi'] as const;
    expect(agentsForRouteEndpoint('messages', targets, kimiSource, map)).toEqual(['claude']);
    expect(agentsForRouteEndpoint('responses', targets, kimiSource, map)).toEqual(['codex']);
    expect(agentsForRouteEndpoint('chat_completions', targets, kimiSource, map)).toEqual(['kimi']);
    expect(representativeAgentForRouteEndpoint('messages', targets, kimiSource, map)).toBe('claude');
    expect(representativeAgentForRouteEndpoint('chat_completions', targets, kimiSource, map)).toBe('kimi');
    expect(eligibilityForRouteEndpoint('messages', targets, kimiSource, map)?.kind).toBe('ready');
  });

  it('prefers a canApply writer when several agents share a surface', () => {
    const map = new Map<string, PlanEligibility>([
      [planFanoutKey({ source: kimiSource, targetAgentId: 'kimi' }), readyEligibility(false, {
        analysis: analysis({ route: 'local_bridge', ruleId: 'codex-subscription-to-kimi-v1' }),
      })],
      [planFanoutKey({ source: kimiSource, targetAgentId: 'dsh' }), readyEligibility(true, {
        analysis: analysis({ route: 'local_bridge', ruleId: 'codex-subscription-to-dsh-v1' }),
      })],
    ]);
    expect(representativeAgentForRouteEndpoint(
      'chat_completions',
      ['kimi', 'dsh'],
      kimiSource,
      map,
    )).toBe('dsh');
  });
});
