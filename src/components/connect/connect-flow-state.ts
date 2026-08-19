/**
 * ConnectFlowDialog 纯状态机（无 React）。
 * 只依赖 connect-flow/types 与既有 lib 类型；deps 由调用方注入。
 */
import type { Account, AgentId, Provider } from '@/lib/types';
import type { AdapterApplyPlan, AdapterApplyResult, AdapterProfile } from '@/lib/api/adapter';
import type {
  ConnectFlowDeps,
  ConnectFlowEntry,
  ConnectOutcome,
  ConnectSourceRef,
  PlanEligibility,
  SourceOption,
} from '@/lib/connect-flow/types';
import { connectSourceKey, planFanoutKey } from '@/lib/connect-flow/types';
import type { AdapterReusePath } from '@/lib/backend/contracts/adapter';
import type { TranslateFn } from '@/lib/i18n';

export type ConnectFlowStep = 'select' | 'preview' | 'result';
export type ConnectFlowBusy = 'idle' | 'applying' | 'switching';
export type ConnectFlowPreviewKind = 'apply' | 'switch';

export type ConnectFlowResultView =
  | {
      kind: 'applied';
      result: AdapterApplyResult;
      isCurrent: boolean;
      refreshFailed: boolean;
    }
  | {
      kind: 'switched';
      ref: ConnectSourceRef;
      agentId: AgentId;
      refreshFailed: boolean;
    }
  | {
      kind: 'failed';
      action: 'apply' | 'switch';
      error: string;
    };

export type PresetResolution =
  | { status: 'ok' }
  | { status: 'none' }
  | { status: 'invalid'; message: string }
  | { status: 'deleted'; message: string };

export type ConnectFlowEmptyKind =
  | { kind: 'none' }
  | { kind: 'wallet_empty' }
  | { kind: 'all_infeasible' }
  | { kind: 'partial_load_error'; message: string }
  | { kind: 'preset_invalid'; message: string }
  | { kind: 'preset_deleted'; message: string };

export type PoolLoadState = 'idle' | 'loading' | 'ready' | 'partial' | 'error';

export interface BoundPlan {
  generation: number;
  source: ConnectSourceRef;
  targetAgentId: AgentId;
  plan: AdapterApplyPlan;
}

export interface ConnectFlowState {
  entry: ConnectFlowEntry;
  step: ConnectFlowStep;
  selectedSource: ConnectSourceRef | null;
  selectedTargetAgentId: AgentId | null;
  /** 选择切换后递增；绑定 plan 带此代次，失配即 stale。 */
  selectionGeneration: number;
  previewKind: ConnectFlowPreviewKind | null;
  boundPlan: BoundPlan | null;
  busy: ConnectFlowBusy;
  result: ConnectFlowResultView | null;
  lastError: string | null;
}

export type ConnectFlowEvent =
  | { type: 'reset'; entry: ConnectFlowEntry }
  | { type: 'select_source'; option: SourceOption }
  | { type: 'select_target'; agentId: AgentId; sourceAgentId: AgentId | null }
  | { type: 'enter_preview'; option?: SourceOption | null; eligibility?: PlanEligibility }
  | { type: 'back_to_select' }
  | { type: 'begin_apply' }
  | { type: 'begin_switch' }
  | { type: 'apply_succeeded'; generation: number; result: AdapterApplyResult }
  | { type: 'apply_failed'; generation: number; error: string }
  | { type: 'apply_rejected_stale'; generation: number }
  | { type: 'switch_succeeded'; ref: ConnectSourceRef; agentId: AgentId }
  | { type: 'switch_failed'; error: string }
  | { type: 'refresh_failed' }
  | { type: 'retry_from_result' };

export const RESULT_ACTIVE = '已生效';
export const RESULT_APPLIED = '已应用';
export const RESULT_SWITCHED = '已切换';
export const REFRESH_FAILED_APPLIED = '已应用，但列表刷新失败';
export const REFRESH_FAILED_SWITCHED = '已切换，但列表刷新失败';

const ILLEGAL_SOURCE_MESSAGE = '来源参数非法';
const DELETED_SOURCE_MESSAGE = '来源已删除';
const ILLEGAL_TARGET_MESSAGE = '目标 Agent 参数非法';
export const GENERATED_SOURCE_REUSE_MESSAGE = '该连接由本机路由生成，不能再次用于其他 Agent';

export function sameSourceRef(left: ConnectSourceRef | null, right: ConnectSourceRef | null): boolean {
  if (!left || !right) return false;
  return left.kind === right.kind && left.id === right.id;
}

export function isBlankId(id: string | null | undefined): boolean {
  return !id || id.trim().length === 0;
}

export function isIllegalSourceRef(ref: ConnectSourceRef | null | undefined): boolean {
  if (!ref) return true;
  if (ref.kind !== 'account' && ref.kind !== 'provider') return true;
  return isBlankId(ref.id);
}

export function lookupSourceRecord(
  ref: ConnectSourceRef,
  accounts: readonly Account[],
  providers: readonly Provider[],
): { agentId: AgentId; account?: Account; provider?: Provider } | null {
  if (ref.kind === 'account') {
    const account = accounts.find((item) => item.id === ref.id);
    return account ? { agentId: account.agentId, account } : null;
  }
  const provider = providers.find((item) => item.id === ref.id);
  return provider ? { agentId: provider.agentId, provider } : null;
}

export function resolvePreset(
  entry: ConnectFlowEntry,
  accounts: readonly Account[],
  providers: readonly Provider[],
  t?: TranslateFn,
): PresetResolution {
  if (entry.mode === 'for-agent') {
    if (isBlankId(entry.targetAgentId)) {
      return { status: 'invalid', message: t ? t('connect.result.illegalTarget') : ILLEGAL_TARGET_MESSAGE };
    }
    return { status: 'ok' };
  }
  if (isIllegalSourceRef(entry.source)) {
    return { status: 'invalid', message: t ? t('connect.result.illegalSource') : ILLEGAL_SOURCE_MESSAGE };
  }
  const found = lookupSourceRecord(entry.source, accounts, providers);
  if (!found) {
    return { status: 'deleted', message: t ? t('connect.result.deletedSource') : DELETED_SOURCE_MESSAGE };
  }
  return { status: 'ok' };
}

export function sourceAgentIdOf(
  entry: ConnectFlowEntry,
  accounts: readonly Account[],
  providers: readonly Provider[],
): AgentId | null {
  if (entry.mode !== 'for-source') return null;
  if (isIllegalSourceRef(entry.source)) return null;
  return lookupSourceRecord(entry.source, accounts, providers)?.agentId ?? null;
}

/** for-source 目标网格：排除来源自身所属 Agent。 */
export function excludeOwnAgentTargets(
  candidates: readonly AgentId[],
  sourceAgentId: AgentId | null,
): AgentId[] {
  if (!sourceAgentId) return [...candidates];
  return candidates.filter((id) => id !== sourceAgentId);
}

export function currentTargetAgentId(state: ConnectFlowState): AgentId | null {
  if (state.entry.mode === 'for-agent') return state.entry.targetAgentId;
  return state.selectedTargetAgentId;
}

export function createConnectFlowState(entry: ConnectFlowEntry): ConnectFlowState {
  return {
    entry,
    step: 'select',
    selectedSource: entry.mode === 'for-source' && !isIllegalSourceRef(entry.source) ? entry.source : null,
    selectedTargetAgentId: entry.mode === 'for-agent' && !isBlankId(entry.targetAgentId)
      ? entry.targetAgentId
      : null,
    selectionGeneration: 0,
    previewKind: null,
    boundPlan: null,
    busy: 'idle',
    result: null,
    lastError: null,
  };
}

function bumpSelection(state: ConnectFlowState, patch: Partial<ConnectFlowState>): ConnectFlowState {
  return {
    ...state,
    ...patch,
    selectionGeneration: state.selectionGeneration + 1,
    boundPlan: null,
    previewKind: null,
    lastError: null,
    result: null,
    step: 'select',
  };
}

function isBusy(state: ConnectFlowState): boolean {
  return state.busy !== 'idle';
}

/** 门禁权威：只读 plan.canApply。eligibility.canApply 仅供展示，矛盾时以 plan 为准。 */
export function planEligibilityAllowsApply(eligibility: PlanEligibility | undefined): boolean {
  return eligibility?.kind === 'ready' && eligibility.plan.canApply === true;
}

export function isOptionSelectable(option: SourceOption, eligibility?: PlanEligibility): boolean {
  if (option.state.kind === 'current' || option.state.kind === 'blocked_native') return false;
  if (option.state.kind === 'switchable') return true;
  if (option.state.kind === 'plannable') {
    return planEligibilityAllowsApply(eligibility);
  }
  return false;
}

export function isTargetSelectable(eligibility: PlanEligibility | undefined): boolean {
  return planEligibilityAllowsApply(eligibility);
}

export function isBoundPlanStale(state: ConnectFlowState): boolean {
  if (!state.boundPlan) return true;
  if (state.boundPlan.generation !== state.selectionGeneration) return true;
  if (!sameSourceRef(state.boundPlan.source, state.selectedSource)) return true;
  if (state.boundPlan.targetAgentId !== currentTargetAgentId(state)) return true;
  return false;
}

export function canEnterPreview(
  state: ConnectFlowState,
  option: SourceOption | null | undefined,
  eligibility?: PlanEligibility,
): boolean {
  if (isBusy(state) || state.step !== 'select') return false;
  if (state.entry.mode === 'for-source') {
    const target = currentTargetAgentId(state);
    if (!target || !state.selectedSource) return false;
    return isTargetSelectable(eligibility);
  }
  if (!option) return false;
  if (!sameSourceRef(state.selectedSource, option.ref)) return false;
  if (option.state.kind === 'switchable') return true;
  if (option.state.kind === 'plannable') {
    return planEligibilityAllowsApply(eligibility);
  }
  return false;
}

export function canConfirm(state: ConnectFlowState): boolean {
  if (isBusy(state) || state.step !== 'preview') return false;
  if (state.previewKind === 'switch') {
    return state.selectedSource !== null;
  }
  if (state.previewKind !== 'apply') return false;
  if (isBoundPlanStale(state) || !state.boundPlan) return false;
  return state.boundPlan.plan.canApply === true;
}

export function canClose(state: ConnectFlowState): boolean {
  return !isBusy(state);
}

export function canRetry(state: ConnectFlowState): boolean {
  return state.step === 'result' && state.result?.kind === 'failed' && !isBusy(state);
}

function bindPlanFromEligibility(
  state: ConnectFlowState,
  eligibility: PlanEligibility | undefined,
): BoundPlan | null {
  const source = state.selectedSource;
  const target = currentTargetAgentId(state);
  if (!source || !target) return null;
  if (eligibility?.kind !== 'ready' || eligibility.plan.canApply !== true) return null;
  return {
    generation: state.selectionGeneration,
    source,
    targetAgentId: target,
    plan: eligibility.plan,
  };
}

export function reduceConnectFlow(state: ConnectFlowState, event: ConnectFlowEvent): ConnectFlowState {
  if (event.type === 'reset') {
    return createConnectFlowState(event.entry);
  }

  if (isBusy(state)) {
    switch (event.type) {
      case 'apply_succeeded':
      case 'apply_failed':
      case 'apply_rejected_stale':
      case 'switch_succeeded':
      case 'switch_failed':
      case 'refresh_failed':
        break;
      default:
        return state;
    }
  }

  switch (event.type) {
    case 'select_source': {
      if (event.option.state.kind === 'current' || event.option.state.kind === 'blocked_native') {
        return state;
      }
      if (state.entry.mode === 'for-source') return state;
      if (sameSourceRef(state.selectedSource, event.option.ref) && state.step === 'select') {
        return state;
      }
      return bumpSelection(state, { selectedSource: event.option.ref });
    }
    case 'select_target': {
      if (state.entry.mode !== 'for-source') return state;
      if (event.sourceAgentId && event.agentId === event.sourceAgentId) return state;
      if (isBlankId(event.agentId)) return state;
      if (state.selectedTargetAgentId === event.agentId && state.step === 'select') {
        return state;
      }
      return bumpSelection(state, { selectedTargetAgentId: event.agentId });
    }
    case 'enter_preview': {
      if (state.step !== 'select') return state;
      if (state.entry.mode === 'for-source') {
        const bound = bindPlanFromEligibility(state, event.eligibility);
        if (!bound) return state;
        return {
          ...state,
          step: 'preview',
          previewKind: 'apply',
          boundPlan: bound,
          lastError: null,
        };
      }
      const option = event.option;
      if (!option || !sameSourceRef(state.selectedSource, option.ref)) return state;
      if (option.state.kind === 'switchable') {
        return {
          ...state,
          step: 'preview',
          previewKind: 'switch',
          boundPlan: null,
          lastError: null,
        };
      }
      if (option.state.kind === 'plannable') {
        const bound = bindPlanFromEligibility(state, event.eligibility);
        if (!bound) return state;
        return {
          ...state,
          step: 'preview',
          previewKind: 'apply',
          boundPlan: bound,
          lastError: null,
        };
      }
      return state;
    }
    case 'back_to_select': {
      if (state.step !== 'preview') return state;
      return {
        ...state,
        step: 'select',
        previewKind: null,
        boundPlan: null,
        lastError: null,
      };
    }
    case 'begin_apply': {
      if (!canConfirm(state) || state.previewKind !== 'apply') return state;
      return { ...state, busy: 'applying', lastError: null };
    }
    case 'begin_switch': {
      if (!canConfirm(state) || state.previewKind !== 'switch') return state;
      return { ...state, busy: 'switching', lastError: null };
    }
    case 'apply_succeeded': {
      if (state.busy !== 'applying') return state;
      if (event.generation !== state.selectionGeneration) return state;
      return {
        ...state,
        busy: 'idle',
        step: 'result',
        lastError: null,
        result: {
          kind: 'applied',
          result: event.result,
          isCurrent: event.result.provider.isCurrent === true,
          refreshFailed: false,
        },
      };
    }
    case 'apply_failed': {
      if (state.busy !== 'applying') return state;
      if (event.generation !== state.selectionGeneration) return state;
      return {
        ...state,
        busy: 'idle',
        step: 'result',
        lastError: event.error,
        result: { kind: 'failed', action: 'apply', error: event.error },
      };
    }
    case 'apply_rejected_stale': {
      if (state.busy !== 'applying') return state;
      return {
        ...state,
        busy: 'idle',
        lastError: null,
      };
    }
    case 'switch_succeeded': {
      if (state.busy !== 'switching') return state;
      return {
        ...state,
        busy: 'idle',
        step: 'result',
        lastError: null,
        result: {
          kind: 'switched',
          ref: event.ref,
          agentId: event.agentId,
          refreshFailed: false,
        },
      };
    }
    case 'switch_failed': {
      if (state.busy !== 'switching') return state;
      return {
        ...state,
        busy: 'idle',
        step: 'result',
        lastError: event.error,
        result: { kind: 'failed', action: 'switch', error: event.error },
      };
    }
    case 'refresh_failed': {
      if (!state.result || (state.result.kind !== 'applied' && state.result.kind !== 'switched')) {
        return state;
      }
      return {
        ...state,
        result: { ...state.result, refreshFailed: true },
      };
    }
    case 'retry_from_result': {
      if (!canRetry(state)) return state;
      return {
        ...state,
        step: 'preview',
        result: null,
        lastError: null,
        busy: 'idle',
      };
    }
    default:
      return state;
  }
}

export function formatConnectFlowError(error: unknown, t?: TranslateFn): string {
  if (error && typeof error === 'object' && 'message' in error && typeof error.message === 'string') {
    return error.message;
  }
  if (typeof error === 'string' && error.trim()) return error;
  return t ? t('connect.result.opFailed') : '操作失败';
}

export function connectFlowResultMessage(result: ConnectFlowResultView, t?: TranslateFn): string {
  if (result.kind === 'failed') return result.error;
  if (result.kind === 'applied') {
    if (result.refreshFailed) return t ? t('connect.result.refreshApplied') : REFRESH_FAILED_APPLIED;
    return result.isCurrent
      ? (t ? t('connect.result.active') : RESULT_ACTIVE)
      : (t ? t('connect.result.applied') : RESULT_APPLIED);
  }
  if (result.refreshFailed) return t ? t('connect.result.refreshSwitched') : REFRESH_FAILED_SWITCHED;
  return t ? t('connect.result.switched') : RESULT_SWITCHED;
}

export interface PlanPreviewView {
  title: string;
  experimental: boolean;
  reason: string;
  notes: string[];
}

function reusePathForPlan(plan: AdapterApplyPlan): AdapterReusePath {
  if (plan.reusePath) return plan.reusePath;
  if (plan.analysis.route === 'unsupported') return 'none';
  if (plan.analysis.route === 'local_bridge') return 'local_bridge';
  return 'api_endpoint';
}

function titleForReusePath(reusePath: AdapterReusePath, t?: TranslateFn): string {
  if (reusePath === 'local_bridge') return t ? t('kind.route.localRoute') : '本机路由';
  if (reusePath === 'api_endpoint') return t ? t('kind.route.direct') : '直连';
  if (reusePath === 'native_subscription') return t ? t('kind.route.reuseLogin') : '用这份登录';
  return t ? t('kind.route.unsupported') : '当前不支持';
}

function endReasonWithFullStop(reason: string): string {
  return reason.endsWith('.') ? `${reason.slice(0, -1)}。` : reason;
}

function localTargetLabels(targetAgentId: string): { display: string; short: string } {
  if (targetAgentId === 'claude') return { display: 'Claude Code', short: 'Claude' };
  if (targetAgentId === 'codex') return { display: 'Codex', short: 'Codex' };
  if (targetAgentId === 'pi') return { display: 'Pi', short: 'Pi' };
  return { display: targetAgentId, short: targetAgentId };
}

function sourceHintFromReason(reason: string, t?: TranslateFn): string {
  if (reason.includes('Grok')) return t ? t('connect.preview.sourceGrok') : 'Grok 登录';
  if (reason.includes('Codex')) return t ? t('connect.preview.sourceCodex') : 'Codex / ChatGPT 登录';
  return t ? t('connect.preview.sourceLogin') : '登录';
}

function notesForReusePath(reusePath: AdapterReusePath, t?: TranslateFn): string[] {
  if (reusePath === 'local_bridge') return [t ? t('connect.preview.noteLocalRoute') : '关掉会进托盘，路由继续跑。'];
  if (reusePath === 'api_endpoint') return [t ? t('connect.preview.noteDirect') : '会把这份连接写进目标 Agent。'];
  if (reusePath === 'native_subscription') return [t ? t('connect.preview.noteReuse') : '会把这份官方登录写进目标 Agent。'];
  return [];
}

export function describePlanPreview(plan: AdapterApplyPlan, t?: TranslateFn): PlanPreviewView {
  const reusePath = reusePathForPlan(plan);
  const analysisReason = plan.analysis.reason || plan.reason || '';
  const sourceHint = sourceHintFromReason(analysisReason, t);
  const display = localTargetLabels(plan.targetAgentId).display;
  const reason = reusePath === 'local_bridge'
    ? (t
      ? t('connect.preview.localReason', { source: sourceHint, target: display })
      : `用这份 ${sourceHint}接到 ${display}。`)
    : endReasonWithFullStop(analysisReason);
  return {
    title: titleForReusePath(reusePath, t),
    experimental: plan.analysis.support === 'experimental',
    reason,
    notes: notesForReusePath(reusePath, t),
  };
}

/**
 * Preview footer import hint: only when the selected source is missing or not logged in.
 * Hidden for a for-source live imported ticket, and for a for-agent logged-in imported source.
 */
export function shouldShowPreviewImportHint(input: {
  entry: ConnectFlowEntry;
  option: SourceOption | null;
  accounts?: readonly Account[];
  providers?: readonly Provider[];
}): boolean {
  const source = input.entry.mode === 'for-source'
    ? input.entry.source
    : input.option?.ref ?? null;
  const account = input.option?.account
    ?? (source?.kind === 'account'
      ? input.accounts?.find((item) => item.id === source.id)
      : undefined);
  if (account) return account.tokenValid !== true;
  const provider = input.option?.provider
    ?? (source?.kind === 'provider'
      ? input.providers?.find((item) => item.id === source.id)
      : undefined);
  if (provider) return false;
  return true;
}

export function eligibilityOf(
  eligibilities: ReadonlyMap<string, PlanEligibility>,
  source: ConnectSourceRef | null,
  targetAgentId: AgentId | null,
): PlanEligibility | undefined {
  if (!source || !targetAgentId) return undefined;
  return eligibilities.get(planFanoutKey({ source, targetAgentId }));
}

export function isEligibilityPending(eligibility: PlanEligibility | undefined): boolean {
  return !eligibility || eligibility.kind === 'loading';
}

export interface EmptyKindInput {
  poolState: PoolLoadState;
  poolErrors: { accounts?: unknown; providers?: unknown };
  profilesError?: unknown;
  /** 当前 entry 的 profiles 是否已完成首次加载；未就绪时不做 generated 来源判定。 */
  profilesReady?: boolean;
  profiles?: readonly Pick<AdapterProfile, 'generatedProviderId'>[];
  accounts: readonly Account[];
  providers: readonly Provider[];
  options: readonly SourceOption[];
  eligibilities: ReadonlyMap<string, PlanEligibility>;
  entry: ConnectFlowEntry;
  visibleTargetAgentIds?: readonly AgentId[];
  t?: TranslateFn;
}

function poolFailureMessage(
  errors: { accounts?: unknown; providers?: unknown },
  profilesError?: unknown,
  t?: TranslateFn,
): string {
  const parts: string[] = [];
  if (errors.accounts) parts.push(t ? t('connect.result.partAccounts') : '账户');
  if (errors.providers) parts.push(t ? t('connect.result.partProviders') : '供应商');
  if (profilesError) parts.push(t ? t('connect.result.partProfiles') : '绑定档案');
  if (parts.length === 0) return t ? t('connect.result.poolPartial') : '部分资源加载失败';
  return t
    ? t('connect.result.poolPartialParts', { parts: parts.join('、') })
    : `部分资源加载失败：${parts.join('、')}。请重试，勿将缺失数据当作空列表。`;
}

export function resolveEmptyKind(input: EmptyKindInput): ConnectFlowEmptyKind {
  const preset = resolvePreset(input.entry, input.accounts, input.providers, input.t);
  const poolBroken = input.poolState === 'error' || input.poolState === 'partial'
    || Boolean(input.poolErrors.accounts)
    || Boolean(input.poolErrors.providers)
    || Boolean(input.profilesError);

  if (poolBroken) {
    return { kind: 'partial_load_error', message: poolFailureMessage(input.poolErrors, input.profilesError, input.t) };
  }

  if (preset.status === 'invalid') {
    return { kind: 'preset_invalid', message: preset.message };
  }
  if (preset.status === 'deleted') {
    return { kind: 'preset_deleted', message: preset.message };
  }

  if (
    input.entry.mode === 'for-source'
    && input.profilesReady === true
    && input.profiles
    && isGeneratedAdapterSource(input.entry.source, input.profiles)
  ) {
    return {
      kind: 'preset_invalid',
      message: input.t ? input.t('connect.result.generatedReuse') : GENERATED_SOURCE_REUSE_MESSAGE,
    };
  }

  if (input.poolState !== 'ready') {
    return { kind: 'none' };
  }

  if (input.accounts.length === 0 && input.providers.length === 0) {
    return { kind: 'wallet_empty' };
  }

  if (input.entry.mode === 'for-source') {
    const targets = input.visibleTargetAgentIds ?? [];
    if (targets.length === 0) return { kind: 'all_infeasible' };
    const source = input.entry.source;
    let pending = false;
    let anySelectable = false;
    for (const target of targets) {
      const eligibility = eligibilityOf(input.eligibilities, source, target);
      if (isEligibilityPending(eligibility)) pending = true;
      if (isTargetSelectable(eligibility)) anySelectable = true;
    }
    if (pending) return { kind: 'none' };
    return anySelectable ? { kind: 'none' } : { kind: 'all_infeasible' };
  }

  let pending = false;
  let anySelectable = false;
  let anyCurrent = false;
  const target = input.entry.targetAgentId;
  for (const option of input.options) {
    if (option.state.kind === 'current') {
      anyCurrent = true;
      continue;
    }
    if (option.state.kind === 'switchable') {
      anySelectable = true;
      continue;
    }
    if (option.state.kind === 'plannable') {
      const eligibility = eligibilityOf(input.eligibilities, option.ref, target);
      if (isEligibilityPending(eligibility)) pending = true;
      if (isOptionSelectable(option, eligibility)) anySelectable = true;
    }
  }
  if (pending) return { kind: 'none' };
  if (anyCurrent) return { kind: 'none' };
  return anySelectable ? { kind: 'none' } : { kind: 'all_infeasible' };
}

export function beginConfirm(state: ConnectFlowState): { next: ConnectFlowState; allowed: boolean } {
  if (!canConfirm(state)) {
    return { next: state, allowed: false };
  }
  if (state.previewKind === 'apply') {
    if (isBoundPlanStale(state)) {
      return { next: state, allowed: false };
    }
    return { next: reduceConnectFlow(state, { type: 'begin_apply' }), allowed: true };
  }
  if (state.previewKind === 'switch') {
    return { next: reduceConnectFlow(state, { type: 'begin_switch' }), allowed: true };
  }
  return { next: state, allowed: false };
}

export type SettleConfirmResult =
  | { event: Extract<ConnectFlowEvent, { type: 'apply_succeeded' | 'apply_failed' | 'apply_rejected_stale' | 'switch_succeeded' | 'switch_failed' }>; called: 'apply' | 'switch' | null };

export async function settleConfirm(input: {
  state: ConnectFlowState;
  startedGeneration: number;
  deps: Pick<ConnectFlowDeps, 'apply' | 'switchNative'>;
  option: SourceOption | null;
  t?: TranslateFn;
}): Promise<SettleConfirmResult> {
  const { state, startedGeneration, deps, option, t } = input;

  if (state.busy === 'applying') {
    if (
      startedGeneration !== state.selectionGeneration
      || isBoundPlanStale(state)
      || !state.boundPlan
      || state.boundPlan.plan.canApply !== true
    ) {
      return { event: { type: 'apply_rejected_stale', generation: startedGeneration }, called: null };
    }
    try {
      const result = await deps.apply({
        sourceKind: state.boundPlan.source.kind,
        sourceId: state.boundPlan.source.id,
        targetAgentId: state.boundPlan.targetAgentId,
      });
      return { event: { type: 'apply_succeeded', generation: startedGeneration, result }, called: 'apply' };
    } catch (error) {
      return {
        event: { type: 'apply_failed', generation: startedGeneration, error: formatConnectFlowError(error, t) },
        called: 'apply',
      };
    }
  }

  if (state.busy === 'switching') {
    if (!option || option.state.kind !== 'switchable') {
      return {
        event: { type: 'switch_failed', error: t ? t('connect.result.cannotSwitch') : '当前项不可切换' },
        called: null,
      };
    }
    try {
      await deps.switchNative(option);
      return {
        event: { type: 'switch_succeeded', ref: option.ref, agentId: option.agentId },
        called: 'switch',
      };
    } catch (error) {
      return { event: { type: 'switch_failed', error: formatConnectFlowError(error, t) }, called: 'switch' };
    }
  }

  return { event: { type: 'apply_rejected_stale', generation: startedGeneration }, called: null };
}

export async function notifyConnectionChangedSafe(
  outcome: ConnectOutcome,
  onConnectionChanged: (outcome: ConnectOutcome) => void | Promise<void>,
): Promise<boolean> {
  try {
    await onConnectionChanged(outcome);
    return true;
  } catch {
    return false;
  }
}

export function outcomeFromResult(result: ConnectFlowResultView): ConnectOutcome | null {
  if (result.kind === 'applied') return { kind: 'applied', result: result.result };
  if (result.kind === 'switched') return { kind: 'switched', ref: result.ref, agentId: result.agentId };
  return null;
}

export function findOption(
  options: readonly SourceOption[],
  ref: ConnectSourceRef | null,
): SourceOption | null {
  if (!ref) return null;
  return options.find((item) => sameSourceRef(item.ref, ref)) ?? null;
}

export function connectFlowEntryKey(entry: ConnectFlowEntry | null): string | null {
  if (!entry) return null;
  return entry.mode === 'for-agent'
    ? `for-agent:${entry.targetAgentId}`
    : `for-source:${entry.source.kind}:${entry.source.id}`;
}

/** 状态机 entry 与当前打开的 entry 不同步（首帧旧会话）。 */
export function isConnectFlowEntryStale(
  stateEntry: ConnectFlowEntry,
  liveEntry: ConnectFlowEntry | null,
): boolean {
  if (!liveEntry) return true;
  return connectFlowEntryKey(stateEntry) !== connectFlowEntryKey(liveEntry);
}

/** profiles 是否已完成针对当前 entry 的首次加载（含成功或失败）。 */
export function isProfilesReadyForEntry(
  loadedKey: string | null,
  entryKey: string | null,
): boolean {
  return loadedKey !== null && entryKey !== null && loadedKey === entryKey;
}

/** 未就绪或加载失败时不构建 options，避免 generated Provider fail-open。 */
export function canBuildSourceOptions(profilesReady: boolean, profilesError?: unknown): boolean {
  return profilesReady && !profilesError;
}

export function isGeneratedAdapterSource(
  source: ConnectSourceRef,
  profiles: readonly Pick<AdapterProfile, 'generatedProviderId'>[],
): boolean {
  if (source.kind !== 'provider') return false;
  return profiles.some((item) => (
    typeof item.generatedProviderId === 'string'
    && item.generatedProviderId.length > 0
    && item.generatedProviderId === source.id
  ));
}

export function shouldShowSelectSkeleton(input: {
  profilesReady: boolean;
  poolLoading: boolean;
  optionsLength: number;
  targetAgentIdsLength: number;
}): boolean {
  if (!input.profilesReady) return true;
  return input.poolLoading && input.optionsLength === 0 && input.targetAgentIdsLength === 0;
}

/** for-agent 预览步：选中项已不在 options 中则必须退回 select。for-source 的 options 恒为空，不适用。 */
export function shouldRevertPreviewToSelect(input: {
  step: ConnectFlowStep;
  mode: ConnectFlowEntry['mode'];
  selectedSource: ConnectSourceRef | null;
  options: readonly SourceOption[];
}): boolean {
  if (input.step !== 'preview') return false;
  if (input.mode !== 'for-agent') return false;
  return findOption(input.options, input.selectedSource) === null;
}

export const PREVIEW_SELECTION_STALE_MESSAGE = '所选凭据已变化，请返回重新选择';

/**
 * 渲染层同步判定：预览步选中来源已失效时，确认必须立刻禁用（不能等 effect 回退）。
 * for-agent：options 中找不到 selectedSource；for-source：pool 中找不到固定来源。
 */
export function isPreviewInvalid(input: {
  state: ConnectFlowState;
  options: readonly SourceOption[];
  accounts: readonly Account[];
  providers: readonly Provider[];
}): boolean {
  const { state } = input;
  if (state.step !== 'preview') return false;
  if (state.entry.mode === 'for-agent') {
    if (state.previewKind !== 'apply' && state.previewKind !== 'switch') return false;
    return findOption(input.options, state.selectedSource) === null;
  }
  const source = state.selectedSource ?? state.entry.source;
  if (!source || isIllegalSourceRef(source)) return true;
  return lookupSourceRecord(source, input.accounts, input.providers) === null;
}

/** 与 React ref 同构的确认占锁，同步消除双击窗口。 */
export function tryAcquireConfirmLock(lock: { current: boolean }): boolean {
  if (lock.current) return false;
  lock.current = true;
  return true;
}

export function releaseConfirmLock(lock: { current: boolean }): void {
  lock.current = false;
}

export function splitSourceOptions(options: readonly SourceOption[]): {
  native: SourceOption[];
  cross: SourceOption[];
} {
  const native: SourceOption[] = [];
  const cross: SourceOption[] = [];
  for (const option of options) {
    if (option.group === 'native') native.push(option);
    else cross.push(option);
  }
  return { native, cross };
}

export function fanoutRequestsForAgent(
  options: readonly SourceOption[],
  targetAgentId: AgentId,
): { source: ConnectSourceRef; targetAgentId: AgentId }[] {
  return options
    .filter((option) => option.state.kind === 'plannable')
    .map((option) => ({ source: option.ref, targetAgentId }));
}

export function fanoutRequestsForSource(
  source: ConnectSourceRef,
  targetAgentIds: readonly AgentId[],
): { source: ConnectSourceRef; targetAgentId: AgentId }[] {
  return targetAgentIds.map((targetAgentId) => ({ source, targetAgentId }));
}

export function guideTargetAgentId(state: ConnectFlowState): AgentId | null {
  return currentTargetAgentId(state);
}

export { connectSourceKey, planFanoutKey };
