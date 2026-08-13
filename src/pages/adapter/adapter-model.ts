import { AGENT_MAP } from '@/config/agents';
import { mergeConnectionEntries, type ConnectionEntry } from '@/pages/connections/connection-model';
import type {
  Account,
  AgentId,
  Provider,
} from '@/lib/types';
import type {
  AdapterAction,
  AdapterApplyResult,
  AdapterApplyPlan,
  AdapterBridgeRuntimeStatus,
  AdapterBridgeRuntimeState,
  AdapterProfile,
  AdapterProfileStatus,
  AdapterRouteAnalysis,
  AdapterSupport,
} from '@/lib/backend/contracts/adapter';

export type AdapterResourceLoadState = 'loading' | 'ready' | 'partial' | 'error';

export type AdapterResourceErrors = Partial<Record<'accounts' | 'providers' | 'profiles', unknown>> & {
  bridgeStatuses: Record<string, unknown>;
};

export type AdapterPageResources = {
  entries: ConnectionEntry[];
  profiles: AdapterProfile[];
  bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus>;
  errors: AdapterResourceErrors;
  connectionState: Exclude<AdapterResourceLoadState, 'loading'>;
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

/** A later poll/read failure must not invent connectivity or erase the last known port. */
export function unavailableBridgeStatusForPoll(
  profile: AdapterProfile,
  previous?: AdapterBridgeRuntimeStatus,
): AdapterBridgeRuntimeStatus {
  return {
    profileId: profile.id,
    state: 'error',
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

export function routeLabel(route: AdapterRouteAnalysis['route']): string {
  if (route === 'native_endpoint') return '原生端点';
  if (route === 'local_bridge') return '需要本地代理';
  if (route === 'config_sync') return '直接同步';
  return '当前不支持';
}

export function supportBadge(support: AdapterSupport): { label: string; variant: 'success' | 'warning' | 'default' } {
  if (support === 'stable') return { label: '稳定规则', variant: 'success' };
  if (support === 'experimental') return { label: '实验规则', variant: 'warning' };
  // Neutral, not a fault state: unsupported is a gate conclusion, not a red error.
  return { label: '当前不支持', variant: 'default' };
}

export function futureAvailability(route: AdapterRouteAnalysis['route']): string | null {
  // Applyable routes (native_endpoint / local_bridge) must not claim "later".
  // Preview-only config_sync still surfaces the future-write notice.
  if (route === 'config_sync') return '配置写入后续开放';
  return null;
}

/** The backend's explicit capability flag is the sole apply gate. */
export function canApplyAdapterPlan(plan: AdapterApplyPlan | null): boolean {
  return plan?.canApply === true;
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

/** Neutral unsupported presentation: reason, gate, and safe alternatives only. */
export function unsupportedPresentation(
  analysis: AdapterRouteAnalysis,
  /** Present so callers can pass the paired plan; canApply is always forced false. */
  plan?: AdapterApplyPlan | null,
): {
  headline: string;
  badgeLabel: string;
  reason: string;
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
    badgeLabel: '当前不支持',
    reason: analysis.reason,
    gateLines: subscription
      ? [
          '门禁：当前未通过上游授权、条款与协议兼容性验证。',
          'plan.canApply=false：不会创建适配、启动 Bridge，也没有“强制继续”。',
          '不会把 Codex / ChatGPT 订阅凭据写入 Claude，也不会在界面或日志展示原始凭据。',
          ...(wireWouldApply ? ['检测到异常 plan.canApply=true，界面仍保持只读并拒绝应用。'] : []),
        ]
      : [
          '门禁：当前没有可执行的适配规则，或规则尚未通过验证。',
          'plan.canApply=false：不会写入配置、启动本机服务或改变当前连接。',
          ...(wireWouldApply ? ['检测到异常 plan.canApply=true，界面仍保持只读并拒绝应用。'] : []),
        ],
    alternatives: subscription
      ? [
          '在 Claude Code 使用自身官方登录完成授权。',
          '改用已支持的 API Key 来源（例如 Kimi Code 会员 → Claude 原生端点）。',
          '继续使用当前 Codex / ChatGPT 连接本身，不跨 Agent 复用订阅凭据。',
        ]
      : [
          '继续使用原连接。',
          '改用目标 Agent 自身登录。',
          '更换已支持的来源与目标组合（例如已支持 API Key 路径）。',
        ],
    safetyNote: '当前不支持不等于连接失效；本次分析只读，不会读取或显示凭据。',
    canApply: false,
  };
}

/** Source selection hint: kind + auth health only; never credential material. */
export function sourceStatusHint(entry: Pick<ConnectionEntry, 'kind' | 'authHealth' | 'authStatus'>): string {
  const kind = entry.kind === 'oauth' ? '官方登录' : 'API Key';
  const health = entry.authHealth
    ? ({
        verified: '已连接 · 已验证',
        renewable: '已连接 · 可续期（未验证）',
        configured: '已配置 · 未验证',
        needs_login: '需要重新登录 / 继续授权',
        unknown: '状态未知',
        missing: '未登录',
      } as const)[entry.authHealth]
    : entry.authStatus === 'expired'
      ? '需要重新登录 / 继续授权'
      : entry.authStatus === 'none'
        ? '未登录'
        : entry.authStatus === 'expiring'
          ? '即将过期'
          : '已连接';
  return `${kind} · ${health} · 仅引用 connectionId，不展示凭据`;
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
  return `${sourceKindLabel(entry.source)} · ${AGENT_MAP[entry.agentId]?.name ?? entry.agentId} · ${entry.title}${current} · ${maskedIdSuffix(entry.id)}`;
}

export function adapterProfileRecordLabel(profile: AdapterProfile): string {
  return `${sourceKindLabel(profile.sourceKind)} · ${maskedIdSuffix(profile.sourceId)} → ${AGENT_MAP[profile.targetAgentId]?.name ?? profile.targetAgentId}`;
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
  return error instanceof Error ? error.message : fallback;
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

/** Never interpolate a secret plan value into the page. */
export function adapterPlanChangeLabel(change: AdapterApplyPlan['changes'][number]): string {
  return change.secret
    ? `${change.target} · ${change.field}：引用已保存 Connection（不会显示）`
    : `${change.target} · ${change.field}：${change.value ?? '保持默认'}`;
}

/** Secret actions are descriptive references, not a credential display. */
export function adapterActionLabel(action: AdapterAction): string {
  return `${action.description}${action.secret ? '（引用已保存 Connection）' : action.value ? `：${action.value}` : ''}`;
}

export function resourceFailureMessage(errors: AdapterResourceErrors): string | null {
  const failed = [
    errors.accounts ? '账户' : null,
    errors.providers ? 'Provider' : null,
  ].filter(Boolean);
  return failed.length ? `部分连接资源未能加载：${failed.join('、')}。已保留其余可用数据。` : null;
}

export function targetAgentName(agentId: AgentId): string {
  return AGENT_MAP[agentId]?.name ?? agentId;
}
