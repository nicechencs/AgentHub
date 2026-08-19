import { agentDisplayName } from '@/config/agents';
import { authHealthLabel } from '@/lib/backend/contracts/auth-state';
import type { ConnectionEntry } from '@/lib/connection-entry';
import type { TranslateFn } from '@/lib/i18n';
import type { AgentId } from '@/lib/types';
import type {
  AdapterAction,
  AdapterApplyPlan,
  AdapterApplyResult,
  AdapterProfile,
  AdapterRouteAnalysis,
  AdapterSupport,
} from '@/lib/backend/contracts/adapter';

export function routeLabel(route: AdapterRouteAnalysis['route'], t?: TranslateFn): string {
  if (route === 'native_endpoint') return t ? t('routes.create.route.nativeEndpoint') : '原生端点';
  if (route === 'local_bridge') return t ? t('routes.create.route.localBridge') : '需要本地代理';
  if (route === 'config_sync') return t ? t('routes.create.route.configSync') : '直接同步';
  return t ? t('routes.create.route.unsupported') : '当前不支持';
}

/** Table column copy for the projection path. Credential family is a separate column. */
export function adapterTableRouteLabel(route: AdapterRouteAnalysis['route'], t?: TranslateFn): string {
  if (route === 'local_bridge') return t ? t('routes.create.tableLocalBridge') : '本地协议转换';
  return routeLabel(route, t);
}

export function supportBadge(support: AdapterSupport, t?: TranslateFn): { label: string; variant: 'success' | 'warning' | 'default' } {
  if (support === 'stable') return { label: t ? t('routes.create.support.stable') : '稳定规则', variant: 'success' };
  if (support === 'experimental') return { label: '', variant: 'default' };
  // Neutral, not a fault state: unsupported is a gate conclusion, not a red error.
  return { label: t ? t('routes.create.support.unsupported') : '当前不支持', variant: 'default' };
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
}, t?: TranslateFn): {
  title: string;
  badgeLabel: string;
  badgeVariant: 'success' | 'warning' | 'default' | 'info';
  nextStep: string;
} {
  if (input.authIncomplete) {
    return {
      title: t ? t('routes.create.preview.authTitle') : '先完成授权',
      badgeLabel: t ? t('routes.create.preview.authBadge') : '待授权',
      badgeVariant: 'warning',
      nextStep: t ? t('routes.create.preview.authNext') : '到 Connections 完成授权。',
    };
  }
  if (input.canApply && input.route === 'local_bridge') {
    return {
      title: t ? t('routes.create.preview.localTitle') : '可接入 · 本机路由',
      badgeLabel: t ? t('routes.create.preview.applyBadge') : '可应用',
      badgeVariant: 'success',
      nextStep: t ? t('routes.create.preview.localNext') : '确认后创建本机路由，需保持托盘运行。',
    };
  }
  if (input.canApply && (input.route === 'native_endpoint' || input.route === 'config_sync')) {
    return {
      title: t ? t('routes.create.preview.directTitle') : '可接入 · 直接写入',
      badgeLabel: t ? t('routes.create.preview.applyBadge') : '可应用',
      badgeVariant: 'success',
      nextStep: t ? t('routes.create.preview.directNext') : '确认后写入目标配置。',
    };
  }
  if (input.route === 'config_sync') {
    return {
      title: t ? t('routes.create.preview.previewTitle') : '仅可预览',
      badgeLabel: t ? t('routes.create.preview.previewBadge') : '仅预览',
      badgeVariant: 'warning',
      nextStep: t ? t('routes.create.preview.previewNext') : '暂不支持一键应用。',
    };
  }
  return {
    title: routeLabel(input.route, t),
    badgeLabel: t ? t('routes.create.preview.previewBadge') : '仅预览',
    badgeVariant: 'default',
    nextStep: t ? t('routes.create.preview.previewNext') : '暂不支持一键应用。',
  };
}

export function adapterServiceImpactLabel(
  impact: AdapterApplyPlan['serviceImpact'] | null | undefined,
): string {
  return impact === 'requires_local_bridge'
    ? '本机路由'
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
export function adapterApplyCommit(result: Pick<AdapterApplyResult, 'profile'>, t?: TranslateFn): {
  successMessage: string;
  shouldProbeBridge: boolean;
  shouldRefresh: true;
} {
  return {
    successMessage: result.profile.route === 'local_bridge'
      ? (t ? t('routes.create.apply.localCreated') : '本地桥接已创建并启动，Codex Connection 已切换。')
      : (t ? t('routes.create.apply.applied') : '适配已应用。'),
    shouldProbeBridge: result.profile.route === 'local_bridge',
    shouldRefresh: true,
  };
}

/** Stable but non-identifying enough to distinguish same-titled connections. */
export function maskedIdSuffix(id: string): string {
  const suffix = id.trim().slice(-4);
  return suffix ? `…${suffix}` : '…';
}

export function sourceKindLabel(sourceKind: 'account' | 'provider', t?: TranslateFn): string {
  return sourceKind === 'account'
    ? (t ? t('routes.create.sourceKind.account') : '账户')
    : (t ? t('routes.create.sourceKind.provider') : 'Provider');
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
