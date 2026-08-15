import { agentDisplayName } from '@/config/agents';
import {
  CONNECTION_KIND_FILTERS,
  connectionKindFilterLabel,
  connectionKindFromAdapterProfileMode,
  connectionKindLabel,
  parseConnectionKindFilter,
  type ConnectionKind,
  type ConnectionKindFilter,
} from '@/lib/connection-kind';
import { authHealthLabel } from '@/lib/backend/contracts/auth-state';
import { mergeConnectionEntries, type ConnectionEntry } from '@/lib/connection-entry';
import {
  BRIDGES_NAV_LABEL,
  BRIDGES_PATH,
  bridgesHrefForProfile,
  legacyBridgesRedirectTo,
} from '@/lib/bridges-path';
import type {
  Account,
  AgentId,
  Provider,
} from '@/lib/types';
import {
  AdapterCommandError,
  isAdapterErrorCodeRetryable,
  type AdapterAction,
  type AdapterApplyResult,
  type AdapterApplyPlan,
  type AdapterBridgeRuntimeStatus,
  type AdapterBridgeRuntimeState,
  type AdapterProfile,
  type AdapterProfileMode,
  type AdapterProfileStatus,
  type AdapterRouteAnalysis,
  type AdapterSupport,
} from '@/lib/backend/contracts/adapter';

/**
 * Adapter page filter uses the shared connection-kind taxonomy (`all|oauth|apikey`).
 * Wire profile mode still uses backend `api|oauth`; map at edges only.
 */
export const ADAPTER_CREDENTIAL_FILTERS = CONNECTION_KIND_FILTERS.map((item) => item.value);
export type AdapterCredentialFilter = ConnectionKindFilter;

/** Legacy alias: old `?tab=api|oauth` deep links map onto the credential filter. */
export type AdapterTab = Exclude<AdapterCredentialFilter, 'all'>;

/**
 * Page filter. Missing / unknown values default to all.
 * Accepts legacy `?tab=api|oauth` and normalizes `api` → `apikey`.
 */
export function parseAdapterCredentialFilter(raw: string | null | undefined): AdapterCredentialFilter {
  return parseConnectionKindFilter(raw);
}

/** @deprecated Prefer {@link parseAdapterCredentialFilter}; unknown values now default to `all`. */
export function parseAdapterTab(raw: string | null | undefined): AdapterCredentialFilter {
  return parseAdapterCredentialFilter(raw);
}

export function adapterCredentialFilterLabel(filter: AdapterCredentialFilter): string {
  return connectionKindFilterLabel(filter);
}

export function adapterTabLabel(tab: AdapterTab | AdapterCredentialFilter): string {
  return adapterCredentialFilterLabel(tab === 'all' ? 'all' : tab);
}

export const BRIDGES_PAGE_TITLE = '本机桥';
export const BRIDGES_PAGE_DESCRIPTION = '本机协议转换 · 仅 127.0.0.1';
export const BRIDGES_PAGE_DESCRIPTION_TIP =
  '凭据在 Connections，不展示不复制。多数连接不需要本机转发。需保持托盘运行。日志不记请求正文。';
export const BRIDGES_EMPTY_TITLE = '没有本机桥';
export const BRIDGES_EMPTY_DESCRIPTION =
  '多数连接不需要本机转发。只有协议对不上时才会在这台电脑上开一层转换。若刚完成需要转发的绑定，到 Dashboard 看对应工具上的桥状态。';
export const BRIDGES_WALLET_WITHOUT_RUNTIME_TITLE = '钱包里有本机桥绑定，但找不到运行时';
export const BRIDGES_WALLET_WITHOUT_RUNTIME_DESCRIPTION = '可重试读取。不是「没有本机桥」。';
export { BRIDGES_NAV_LABEL, BRIDGES_PATH, bridgesHrefForProfile, legacyBridgesRedirectTo };

/** Unknown or missing `?profile=` stays on the list; do not toast. */
export function resolveBridgesProfileQuery(
  profileId: string | null | undefined,
  profiles: readonly { id: string }[],
): string | null {
  if (!profileId) return null;
  return profiles.some((profile) => profile.id === profileId) ? profileId : null;
}
export const BRIDGES_MUTATION_FAILURE = '本机桥操作失败';

export function adapterPageDescription(): string {
  return BRIDGES_PAGE_DESCRIPTION;
}

export function adapterTabDescription(_tab?: AdapterTab | AdapterCredentialFilter): string {
  return adapterPageDescription();
}

export function connectionKindForFilter(filter: Exclude<AdapterCredentialFilter, 'all'>): ConnectionKind {
  return filter;
}

export function connectionKindForTab(tab: AdapterTab): ConnectionKind {
  return connectionKindForFilter(tab);
}

export function adapterCredentialKindLabel(mode: AdapterProfileMode): string {
  return connectionKindLabel(connectionKindFromAdapterProfileMode(mode));
}

export function filterProfilesByMode<T extends { mode?: AdapterProfileMode | null }>(
  profiles: readonly T[],
  mode: AdapterProfileMode,
): T[] {
  return profiles.filter((profile) => profile.mode === mode);
}

export function filterProfilesByCredential<T extends { mode?: AdapterProfileMode | null }>(
  profiles: readonly T[],
  filter: AdapterCredentialFilter,
): T[] {
  if (filter === 'all') return [...profiles];
  // Page filter is `apikey`; profile wire mode is still `api`.
  const mode: AdapterProfileMode = filter === 'oauth' ? 'oauth' : 'api';
  return filterProfilesByMode(profiles, mode);
}

export type AdapterResourceLoadState = 'loading' | 'ready' | 'partial' | 'error';

export type AdapterResourceErrors = Partial<Record<'accounts' | 'providers' | 'profiles', unknown>> & {
  bridgeStatuses: Record<string, unknown>;
};

export type AdapterPageResources = {
  entries: ConnectionEntry[];
  profiles: AdapterProfile[];
  bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus>;
  errors: AdapterResourceErrors;
  connectionState: AdapterResourceLoadState;
  profileState: Exclude<AdapterResourceLoadState, 'loading' | 'partial'>;
};

export type AdapterResourceLoaders = {
  listAccounts: () => Promise<Account[]>;
  listProviders: () => Promise<Provider[]>;
  listProfiles: () => Promise<AdapterProfile[]>;
  getBridgeStatus: (profileId: string) => Promise<AdapterBridgeRuntimeStatus>;
};

export const ADAPTER_BRIDGE_STATUS_POLL_MS = 4_000;

export function adapterBridgeProfilesToPoll(
  profiles: AdapterProfile[],
  bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus>,
): AdapterProfile[] {
  return profiles.filter((profile) => shouldPollAdapterBridgeStatus(profile, bridgeStatuses[profile.id]));
}

export function applyAdapterBridgeStatusPoll(
  current: AdapterPageResources,
  targets: AdapterProfile[],
  results: PromiseSettledResult<AdapterBridgeRuntimeStatus>[],
): AdapterPageResources {
  const bridgeStatuses = { ...current.bridgeStatuses };
  const bridgeStatusErrors = { ...current.errors.bridgeStatuses };
  results.forEach((result, index) => {
    const profile = targets[index];
    if (!profile) return;
    if (result.status === 'fulfilled') {
      bridgeStatuses[profile.id] = result.value;
      delete bridgeStatusErrors[profile.id];
      return;
    }
    bridgeStatuses[profile.id] = unavailableBridgeStatusForPoll(profile, bridgeStatuses[profile.id]);
    bridgeStatusErrors[profile.id] = result.reason;
  });
  return {
    ...current,
    bridgeStatuses,
    errors: { ...current.errors, bridgeStatuses: bridgeStatusErrors },
  };
}

function isFulfilled<T>(result: PromiseSettledResult<T>): result is PromiseFulfilledResult<T> {
  return result.status === 'fulfilled';
}

function unavailableBridgeStatus(profile: AdapterProfile): AdapterBridgeRuntimeStatus {
  return {
    profileId: profile.id,
    state: 'error',
    port: profile.localPort ?? null,
    endpoint: null,
    startedAt: null,
    upstreamStatus: 'unavailable',
  };
}

/**
 * A later poll/read failure must not invent connectivity or erase the last
 * known port / state. `error` is only a placeholder when the runtime was never
 * observed — it is not a start failure.
 */
export function unavailableBridgeStatusForPoll(
  profile: AdapterProfile,
  previous?: AdapterBridgeRuntimeStatus,
): AdapterBridgeRuntimeStatus {
  return {
    profileId: profile.id,
    state: previous?.state ?? 'error',
    port: previous?.port ?? profile.localPort ?? null,
    endpoint: previous?.endpoint ?? null,
    startedAt: previous?.startedAt ?? null,
    upstreamStatus: 'unavailable',
  };
}

/**
 * Loads each page resource independently. A failed pool must never erase a
 * successfully loaded pool (in particular, profiles must not look empty when
 * their request failed). Runtime bridge inspection is intentionally best
 * effort: a status failure still returns every persisted profile.
 */
export async function loadAdapterPageResources(loaders: AdapterResourceLoaders): Promise<AdapterPageResources> {
  const [accountsResult, providersResult, profilesResult] = await Promise.allSettled([
    Promise.resolve().then(loaders.listAccounts),
    Promise.resolve().then(loaders.listProviders),
    Promise.resolve().then(loaders.listProfiles),
  ]);

  const accounts = isFulfilled(accountsResult) ? accountsResult.value : [];
  const providers = isFulfilled(providersResult) ? providersResult.value : [];
  const profiles = isFulfilled(profilesResult) ? profilesResult.value : [];
  const localBridgeProfiles = profiles.filter((profile) => profile.route === 'local_bridge');
  const statusResults = await Promise.allSettled(
    localBridgeProfiles.map((profile) => Promise.resolve().then(() => loaders.getBridgeStatus(profile.id))),
  );

  const bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus> = {};
  const bridgeStatusErrors: Record<string, unknown> = {};
  statusResults.forEach((result, index) => {
    const profile = localBridgeProfiles[index];
    if (isFulfilled(result)) {
      bridgeStatuses[profile.id] = result.value;
      return;
    }
    bridgeStatuses[profile.id] = unavailableBridgeStatus(profile);
    bridgeStatusErrors[profile.id] = result.reason;
  });

  const accountError = isFulfilled(accountsResult) ? undefined : accountsResult.reason;
  const providerError = isFulfilled(providersResult) ? undefined : providersResult.reason;
  const profileError = isFulfilled(profilesResult) ? undefined : profilesResult.reason;
  const connectionState = accountError && providerError
    ? 'error'
    : accountError || providerError
      ? 'partial'
      : 'ready';

  return {
    entries: mergeConnectionEntries(accounts, providers),
    profiles,
    bridgeStatuses,
    errors: {
      ...(accountError ? { accounts: accountError } : {}),
      ...(providerError ? { providers: providerError } : {}),
      ...(profileError ? { profiles: profileError } : {}),
      bridgeStatuses: bridgeStatusErrors,
    },
    connectionState,
    profileState: profileError ? 'error' : 'ready',
  };
}

/** Profiles + bridge status only. Connection rows come from the shared pool store. */
export async function loadAdapterProfileResources(
  loaders: Pick<AdapterResourceLoaders, 'listProfiles' | 'getBridgeStatus'>,
): Promise<{
  profiles: AdapterProfile[];
  bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus>;
  profileState: AdapterPageResources['profileState'];
  profileError?: unknown;
  bridgeStatusErrors: Record<string, unknown>;
}> {
  let profiles: AdapterProfile[] = [];
  let profileError: unknown;
  try {
    profiles = await Promise.resolve().then(loaders.listProfiles);
  } catch (error) {
    profileError = error;
  }
  const localBridgeProfiles = profiles.filter((profile) => profile.route === 'local_bridge');
  const statusResults = await Promise.allSettled(
    localBridgeProfiles.map((profile) => Promise.resolve().then(() => loaders.getBridgeStatus(profile.id))),
  );

  const bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus> = {};
  const bridgeStatusErrors: Record<string, unknown> = {};
  statusResults.forEach((result, index) => {
    const profile = localBridgeProfiles[index];
    if (isFulfilled(result)) {
      bridgeStatuses[profile.id] = result.value;
      return;
    }
    bridgeStatuses[profile.id] = unavailableBridgeStatus(profile);
    bridgeStatusErrors[profile.id] = result.reason;
  });

  return {
    profiles,
    bridgeStatuses,
    profileState: profileError ? 'error' : 'ready',
    profileError,
    bridgeStatusErrors,
  };
}

export function routeLabel(route: AdapterRouteAnalysis['route']): string {
  if (route === 'native_endpoint') return '原生端点';
  if (route === 'local_bridge') return '需要本地代理';
  if (route === 'config_sync') return '直接同步';
  return '当前不支持';
}

/** Table column copy for the projection path. Credential family is a separate column. */
export function adapterTableRouteLabel(route: AdapterRouteAnalysis['route']): string {
  if (route === 'local_bridge') return '本地协议转换';
  return routeLabel(route);
}

export function supportBadge(support: AdapterSupport): { label: string; variant: 'success' | 'warning' | 'default' } {
  if (support === 'stable') return { label: '稳定规则', variant: 'success' };
  if (support === 'experimental') return { label: '实验规则', variant: 'warning' };
  // Neutral, not a fault state: unsupported is a gate conclusion, not a red error.
  return { label: '当前不支持', variant: 'default' };
}

/** The backend's explicit capability flag is the sole apply gate. */
export function canApplyAdapterPlan(plan: AdapterApplyPlan | null): boolean {
  return plan?.canApply === true;
}

/**
 * Client-side binding for a plan request. A plan is only safe for preview/apply
 * when this signature still matches the user's current selection.
 */
export type AdapterPlanRequestSignature = {
  sourceKind: 'account' | 'provider';
  sourceId: string;
  targetAgentId: AgentId;
};

export function adapterPlanRequestSignature(input: {
  sourceKind: 'account' | 'provider';
  sourceId: string;
  targetAgentId: AgentId;
}): AdapterPlanRequestSignature {
  return {
    sourceKind: input.sourceKind,
    sourceId: input.sourceId,
    targetAgentId: input.targetAgentId,
  };
}

export function isSameAdapterPlanRequestSignature(
  left: AdapterPlanRequestSignature | null | undefined,
  right: AdapterPlanRequestSignature | null | undefined,
): boolean {
  if (!left || !right) return false;
  return left.sourceKind === right.sourceKind
    && left.sourceId === right.sourceId
    && left.targetAgentId === right.targetAgentId;
}

/**
 * Preview / canApply / confirm must all require an exact signature match.
 * Generation already drops late responses; this blocks any leftover plan state.
 */
export function isAdapterPlanMatchedToSelection(
  plan: AdapterApplyPlan | null | undefined,
  planSignature: AdapterPlanRequestSignature | null | undefined,
  current: AdapterPlanRequestSignature | null | undefined,
): plan is AdapterApplyPlan {
  return Boolean(plan) && isSameAdapterPlanRequestSignature(planSignature, current);
}

/** Apply only when the bound plan matches the current selection and the backend gate is open. */
export function canApplyAdapterSelection(input: {
  plan: AdapterApplyPlan | null | undefined;
  planSignature: AdapterPlanRequestSignature | null | undefined;
  currentSignature: AdapterPlanRequestSignature | null | undefined;
  authIncomplete?: boolean;
}): boolean {
  if (input.authIncomplete) return false;
  if (!isAdapterPlanMatchedToSelection(input.plan, input.planSignature, input.currentSignature)) {
    return false;
  }
  return canApplyAdapterPlan(input.plan);
}

/** Final confirm gate: re-check signature so a stale dialog cannot submit a new request. */
export function canConfirmAdapterApply(input: {
  applyRequest: AdapterPlanRequestSignature | null | undefined;
  plan: AdapterApplyPlan | null | undefined;
  planSignature: AdapterPlanRequestSignature | null | undefined;
  authIncomplete?: boolean;
}): boolean {
  if (!input.applyRequest) return false;
  return canApplyAdapterSelection({
    plan: input.plan,
    planSignature: input.planSignature,
    currentSignature: input.applyRequest,
    authIncomplete: input.authIncomplete,
  });
}

/** Selection is only operable when the key is still present in the visible (filtered/search) list. */
export function resolveAdapterVisibleSourceKey(
  sourceKey: string,
  visibleEntries: readonly { key: string }[],
): string {
  if (!sourceKey) return '';
  return visibleEntries.some((entry) => entry.key === sourceKey) ? sourceKey : '';
}

/**
 * Subscription-bridge candidates (e.g. Codex/ChatGPT → Claude) must stay
 * fail-closed until every gate in the adaptation matrix is green.
 * Prefer structured `gateKind` from core; keep a narrow text fallback for legacy wires.
 */
export function isSubscriptionGateUnsupported(
  analysis: Pick<AdapterRouteAnalysis, 'route' | 'reason' | 'evidence' | 'gateKind' | 'ruleId'>,
): boolean {
  if (analysis.route !== 'unsupported') return false;
  if (analysis.gateKind === 'subscription_candidate') return true;
  if (analysis.ruleId?.startsWith('codex-subscription-to-claude')) return true;
  // Legacy wires without gateKind: last-resort content match only.
  const haystack = [
    analysis.reason,
    ...analysis.evidence.map((item) => `${item.label} ${item.url}`),
  ].join('\n').toLowerCase();
  const mentionsCodexOrChatgpt = /codex|chatgpt|subscription|订阅/.test(haystack);
  const mentionsClaude = /claude/.test(haystack);
  const mentionsGate = /门禁|条款|授权|协议|canapply|unsupported|不支持|暂未/.test(haystack);
  return mentionsCodexOrChatgpt && (mentionsClaude || mentionsGate);
}

/** Neutral unsupported presentation: short reason + few alternatives. */
export function unsupportedPresentation(
  analysis: AdapterRouteAnalysis,
  /** Present so callers can pass the paired plan; canApply is always forced false. */
  plan?: AdapterApplyPlan | null,
): {
  headline: string;
  badgeLabel: string;
  reason: string;
  summary: string;
  gateLines: string[];
  alternatives: string[];
  safetyNote: string;
  canApply: false;
} {
  const subscription = isSubscriptionGateUnsupported(analysis);
  // A buggy wire payload with canApply=true must still fail closed in the UI.
  const wireWouldApply = plan?.canApply === true;
  return {
    headline: subscription ? '当前不支持' : '暂未支持此组合',
    badgeLabel: '不可用',
    reason: analysis.reason,
    summary: '不能应用，不会改动配置。',
    gateLines: subscription
      ? [
          '跨产品授权尚未验证。',
          '不会创建适配或启动桥接。',
          ...(wireWouldApply ? ['异常可应用标记已被忽略。'] : []),
        ]
      : [
          '没有可用路径。',
          '不会写入配置或启动服务。',
          ...(wireWouldApply ? ['异常可应用标记已被忽略。'] : []),
        ],
    alternatives: subscription
      ? [
          '改用 Claude 官方登录。',
          '改用已支持的 API Key，如 Kimi → Claude。',
        ]
      : [
          '改用目标 Agent 自己登录。',
          '换一条已支持的组合。',
        ],
    safetyNote: '',
    canApply: false,
  };
}

/**
 * Compact user-facing outcome. Prefer one title + one short line.
 * Backend fields (canApply / route) stay out of visible copy.
 */
export function adapterPreviewOutcome(input: {
  route: AdapterRouteAnalysis['route'];
  canApply: boolean;
  authIncomplete?: boolean;
}): {
  title: string;
  badgeLabel: string;
  badgeVariant: 'success' | 'warning' | 'default' | 'info';
  nextStep: string;
} {
  if (input.authIncomplete) {
    return {
      title: '先完成授权',
      badgeLabel: '待授权',
      badgeVariant: 'warning',
      nextStep: '到 Connections 完成授权。',
    };
  }
  if (input.canApply && input.route === 'local_bridge') {
    return {
      title: '可接入 · 本地桥接',
      badgeLabel: '可应用',
      badgeVariant: 'success',
      nextStep: '确认后创建本机桥接，需保持托盘运行。',
    };
  }
  if (input.canApply && (input.route === 'native_endpoint' || input.route === 'config_sync')) {
    return {
      title: '可接入 · 直接写入',
      badgeLabel: '可应用',
      badgeVariant: 'success',
      nextStep: '确认后写入目标配置。',
    };
  }
  if (input.route === 'config_sync') {
    return {
      title: '仅可预览',
      badgeLabel: '仅预览',
      badgeVariant: 'warning',
      nextStep: '暂不支持一键应用。',
    };
  }
  return {
    title: routeLabel(input.route),
    badgeLabel: '仅预览',
    badgeVariant: 'default',
    nextStep: '暂不支持一键应用。',
  };
}

export function adapterServiceImpactLabel(
  impact: AdapterApplyPlan['serviceImpact'] | null | undefined,
): string {
  return impact === 'requires_local_bridge'
    ? '本机桥接'
    : '无需本地服务';
}

/** Source health only; kind is already shown as a badge. Never credential material. */
export function sourceStatusHint(entry: Pick<ConnectionEntry, 'kind' | 'authHealth' | 'authStatus'>): string {
  if (entry.authHealth) {
    return authHealthLabel(entry.authHealth);
  }
  if (entry.authStatus === 'expired') return authHealthLabel('needs_login');
  if (entry.authStatus === 'none') return authHealthLabel('missing');
  if (entry.authStatus === 'expiring') return '即将过期';
  return entry.kind === 'oauth' ? '已连接' : '已配置';
}

/** The apply mutation is final; runtime probing is a later best-effort concern. */
export function adapterApplyCommit(result: Pick<AdapterApplyResult, 'profile'>): {
  successMessage: string;
  shouldProbeBridge: boolean;
  shouldRefresh: true;
} {
  return {
    successMessage: result.profile.route === 'local_bridge'
      ? '本地桥接已创建并启动，Codex Connection 已切换。'
      : '适配已应用。',
    shouldProbeBridge: result.profile.route === 'local_bridge',
    shouldRefresh: true,
  };
}

/** Stable but non-identifying enough to distinguish same-titled connections. */
export function maskedIdSuffix(id: string): string {
  const suffix = id.trim().slice(-4);
  return suffix ? `…${suffix}` : '…';
}

export function sourceKindLabel(sourceKind: 'account' | 'provider'): string {
  return sourceKind === 'account' ? '账户' : 'Provider';
}

/** One canonical source label for selection, preview, and confirmation. */
export function sourceLabel(entry: Pick<ConnectionEntry, 'source' | 'id' | 'agentId' | 'title' | 'isCurrent'>): string {
  const current = entry.isCurrent ? ' · 当前' : '';
  // Lead with human names; keep a short id suffix only for same-title disambiguation.
  return `${agentDisplayName(entry.agentId)} · ${entry.title}${current} · ${maskedIdSuffix(entry.id)}`;
}

export function adapterProfileRecordLabel(profile: AdapterProfile): string {
  return `${sourceKindLabel(profile.sourceKind)} · ${maskedIdSuffix(profile.sourceId)} → ${agentDisplayName(profile.targetAgentId)}`;
}

export function adapterProfileStatusLabel(status: AdapterProfileStatus): string {
  if (status === 'active') return '已生效';
  if (status === 'applying') return '应用中';
  return '需要处理';
}

export function adapterBridgeStateLabel(state: AdapterBridgeRuntimeState | undefined): string {
  if (state === 'running') return '运行中';
  if (state === 'starting') return '启动中';
  if (state === 'stopping') return '停止中';
  if (state === 'error') return '运行错误';
  if (state === 'degraded') return '服务降级';
  return '已停止';
}

export function adapterBridgeUpstreamLabel(status: AdapterBridgeRuntimeStatus['upstreamStatus']): string {
  if (status === 'connected') return '已连接';
  if (status === 'stopped') return '已停止';
  if (status === 'degraded') return '降级';
  if (status === 'unavailable') return '不可用';
  return '未知';
}

/** Live local-bridge rows that should keep reading stored runtime status. */
export function shouldPollAdapterBridgeStatus(
  profile: Pick<AdapterProfile, 'route'>,
  status?: AdapterBridgeRuntimeStatus,
): boolean {
  if (profile.route !== 'local_bridge') return false;
  return status?.state === 'running' || status?.state === 'degraded';
}

export function adapterBridgeEndpointLabel(
  profile: AdapterProfile,
  status?: AdapterBridgeRuntimeStatus,
): string | null {
  const port = status?.port ?? profile.localPort;
  return port ? `127.0.0.1:${port}` : null;
}

export function bridgeStatusBadge(state: AdapterBridgeRuntimeState | undefined): {
  label: string;
  variant: 'success' | 'warning' | 'default';
} {
  return {
    label: adapterBridgeStateLabel(state),
    variant: state === 'running'
      ? 'success'
      : state === 'error' || state === 'degraded'
        ? 'warning'
        : 'default',
  };
}

export function profileStatusBadge(status: AdapterProfileStatus): { label: string; variant: 'success' | 'warning' | 'default' } {
  return {
    label: adapterProfileStatusLabel(status),
    variant: status === 'active' ? 'success' : status === 'needs_attention' ? 'warning' : 'default',
  };
}

export function errorMessage(error: unknown, fallback: string): string {
  if (error instanceof AdapterCommandError && error.message.trim()) return error.message;
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === 'string' && error.trim()) return error;
  return fallback;
}

export function isAdapterErrorRetryable(error: unknown): boolean {
  if (error instanceof AdapterCommandError) return error.retryable;
  if (error && typeof error === 'object' && 'retryable' in error && typeof error.retryable === 'boolean') {
    return error.retryable;
  }
  if (error && typeof error === 'object' && 'code' in error && typeof error.code === 'string') {
    return isAdapterErrorCodeRetryable(error.code);
  }
  return false;
}

export function adapterErrorDetails(error: unknown): string | null {
  if (error instanceof AdapterCommandError) {
    const details = error.details?.trim();
    return details || null;
  }
  if (error && typeof error === 'object' && 'details' in error && typeof error.details === 'string') {
    const details = error.details.trim();
    return details || null;
  }
  return null;
}

export function adapterErrorRetryHint(error: unknown): string | null {
  return isAdapterErrorRetryable(error) ? '此错误可重试。' : null;
}

export function adapterFailurePresentation(error: unknown, fallback: string): {
  message: string;
  retryable: boolean;
  hint: string;
} {
  const retryable = isAdapterErrorRetryable(error);
  return {
    message: errorMessage(error, fallback),
    retryable,
    hint: retryable
      ? '可重试；不会自动反复重试。'
      : '不可重试。检查来源连接，或删除后重建。',
  };
}

export type AdapterPageViewState = 'loading' | 'error' | 'empty' | 'choose' | 'preview';

/** Pure page-state mapping keeps the empty/error path testable without a DOM. */
export function adapterPageViewState(input: {
  loading: boolean;
  loadError: unknown;
  entriesCount: number;
  hasSource: boolean;
}): AdapterPageViewState {
  if (input.loading) return 'loading';
  if (input.loadError) return 'error';
  if (input.entriesCount === 0) return 'empty';
  return input.hasSource ? 'preview' : 'choose';
}

/** An old async response must never replace the currently selected preview. */
export function isCurrentAdapterPreviewRequest(generation: number, current: number): boolean {
  return generation === current;
}

/** Empty target list must not reuse a stale Agent id for plan/apply. */
export function resolveAdapterTargetAgentId(
  selected: AgentId | '' | null | undefined,
  available: readonly AgentId[],
): AgentId | '' {
  if (available.length === 0) return '';
  if (selected && available.includes(selected)) return selected;
  return available[0] ?? '';
}

export function canRequestAdapterPlan(input: {
  sourceId?: string | null;
  targetAgentId?: AgentId | '' | null;
}): boolean {
  return Boolean(input.sourceId) && Boolean(input.targetAgentId);
}

/** Keep the last successful profile list when a later listProfiles call fails. */
export function mergeAdapterProfileLoad(
  previous: {
    profiles: AdapterProfile[];
    bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus>;
  },
  next: {
    profiles: AdapterProfile[];
    bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus>;
    profileState: AdapterPageResources['profileState'];
    profileError?: unknown;
    bridgeStatusErrors: Record<string, unknown>;
  },
): {
  profiles: AdapterProfile[];
  bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus>;
  profileState: AdapterPageResources['profileState'];
  profileError?: unknown;
  bridgeStatusErrors: Record<string, unknown>;
} {
  if (!next.profileError) return next;
  if (previous.profiles.length === 0) return next;
  return {
    ...next,
    profiles: previous.profiles,
    bridgeStatuses: Object.keys(next.bridgeStatuses).length > 0
      ? next.bridgeStatuses
      : previous.bridgeStatuses,
  };
}

/** Never interpolate a secret plan value into the page. */
export function adapterPlanChangeLabel(change: AdapterApplyPlan['changes'][number]): string {
  return change.secret
    ? `${change.target} · ${change.field}：使用已保存的密钥`
    : `${change.target} · ${change.field}：${change.value ?? '保持默认'}`;
}

/** Secret actions are descriptive references, not a credential display. */
export function adapterActionLabel(action: AdapterAction): string {
  return `${action.description}${action.secret ? '（使用已保存的密钥）' : action.value ? `：${action.value}` : ''}`;
}

export function resourceFailureMessage(errors: AdapterResourceErrors): string | null {
  const failed = [
    errors.accounts ? '账户' : null,
    errors.providers ? 'Provider' : null,
  ].filter(Boolean);
  return failed.length ? `部分连接未能加载：${failed.join('、')}。其余仍可用。` : null;
}

export function targetAgentName(agentId: AgentId): string {
  return agentDisplayName(agentId);
}

/**
 * Soft agent-colored badge tone for Adapter source rows.
 * Uses brand CSS vars only (no new hex palette).
 */
export function adapterAgentBadgeStyle(color: string): {
  color: string;
  backgroundColor: string;
  boxShadow: string;
} {
  return {
    color,
    backgroundColor: `color-mix(in srgb, ${color} 14%, transparent)`,
    boxShadow: `inset 0 0 0 1px color-mix(in srgb, ${color} 34%, transparent)`,
  };
}
