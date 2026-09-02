/**
 * Pure view-model for Routes board. No React, no IO.
 */
import { agentDisplayName } from '@/config/agents';
import type {
  AdapterBridgeInboundRequest,
  AdapterBridgeRuntimeState,
  AdapterBridgeRuntimeStatus,
  AdapterProfile,
  AdapterProfileStatus,
  DefaultRoutePoolOverview,
  RoutePoolSurface,
} from '@/lib/backend/contracts/adapter';
import {
  LOCAL_ENDPOINT_KINDS,
  localEndpointKindFromPool,
  type LocalEndpointKind,
} from '@/lib/route-endpoints';
import type { AgentId } from '@/lib/types';
import type { TranslateFn } from '@/lib/i18n';
import { isBridgeStopCapable } from '@/pages/bridges/adapter-view-model';
import { localEndpointKindLabel } from '@/pages/bridges/route-pool-view-model';

export const BOARD_INBOUND_SNAPSHOT_LIMIT = 8;
export const BOARD_INBOUND_WINDOW = 20;

/** Same auto-fit card grid as 总览 Agent cards. */
export const BOARD_ROUTE_GRID =
  'grid gap-3 [grid-template-columns:repeat(auto-fit,minmax(190px,1fr))]';

export type RouteBoardAttentionReason =
  | 'unavailable'
  | 'error'
  | 'degraded'
  | 'needs_attention';

export type RouteBoardRecentSummary = {
  lastAt: string | null;
  failedInWindow: number;
  windowSize: number;
  totalRequestCount: number;
  failedRequestCount: number;
};

export type RouteBoardStatusRow = {
  profileId: string;
  name: string;
  targetAgentId: AgentId;
  memberCount: number;
  state: AdapterBridgeRuntimeState | undefined;
  endpoint: string | null;
  upstreamStatus: AdapterBridgeRuntimeStatus['upstreamStatus'];
  lastErrorCode: string | null;
  startedAt: string | null;
  statusUnavailable: boolean;
  profileStatus: AdapterProfileStatus;
  /** Present when this local entry already has a listener to start/stop. */
  profile: AdapterProfile | null;
  recent: RouteBoardRecentSummary;
  needsAttention: boolean;
  attentionReason: RouteBoardAttentionReason | null;
};

export type MergedInboundRow = AdapterBridgeInboundRequest & {
  profileId: string;
  sourceLabel: string;
};

export type BoardFleetSummary = {
  total: number;
  running: number;
  needsAttention: number;
  label: string;
};

/** Shared local-entry master switch (not per-login, not per-endpoint). */
export type LocalEntryControl = {
  profileIds: string[];
  startIds: string[];
  stopIds: string[];
  action: 'start' | 'stop' | null;
  retry: boolean;
  running: boolean;
  starting: boolean;
  stopping: boolean;
  transitioning: boolean;
  /** Process-local restore / start is bringing listeners back. */
  restarting: boolean;
  /** Connection-pool logins exist; the switch still operates if they later fail. */
  hasEnrolledLogins: boolean;
};

export function buildLocalEntryControl(
  profiles: readonly Pick<AdapterProfile, 'id' | 'route' | 'sourceKind' | 'sourceId' | 'targetAgentId' | 'lastErrorCode'>[],
  bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus | undefined>,
  hiddenTargetIds: ReadonlySet<string> = new Set(),
  pools: readonly Pick<DefaultRoutePoolOverview, 'id' | 'targetAgentId' | 'members'>[] = [],
  restarting = false,
): LocalEntryControl {
  const hasEnrolledLogins = pools.some((pool) => (
    !hiddenTargetIds.has(pool.targetAgentId)
    && pool.members.some((member) => member.enabled !== false)
  ));
  const byId = new Map<string, (typeof profiles)[number]>();
  for (const profile of profiles) {
    if (profile.route !== 'local_bridge') continue;
    if (hiddenTargetIds.has(profile.targetAgentId)) continue;
    byId.set(profile.id, profile);
  }
  for (const pool of pools) {
    if (hiddenTargetIds.has(pool.targetAgentId)) continue;
    if (pool.members.length === 0) continue;
    for (const match of profilesForPool(pool, profiles)) {
      byId.set(match.id, match);
    }
  }
  const local = [...byId.values()];
  const profileIds = local.map((profile) => profile.id);
  const stopIds: string[] = [];
  const startIds: string[] = [];
  let retry = false;
  let starting = false;
  let stopping = false;
  for (const profile of local) {
    const state = bridgeStatuses[profile.id]?.state;
    if (state === 'starting') starting = true;
    if (state === 'stopping') stopping = true;
    if (isBridgeStopCapable(state)) {
      stopIds.push(profile.id);
      continue;
    }
    startIds.push(profile.id);
    if (state === 'error' || Boolean(profile.lastErrorCode?.trim())) retry = true;
  }
  const running = stopIds.length > 0;
  const canToggle = running || startIds.length > 0 || hasEnrolledLogins;
  return {
    profileIds,
    startIds,
    stopIds,
    action: !canToggle ? null : (running ? 'stop' : 'start'),
    retry: retry && !running,
    running,
    starting,
    stopping,
    restarting,
    transitioning: starting || stopping || restarting,
    hasEnrolledLogins,
  };
}

/** One card per UI endpoint kind (Responses split into Codex / Grok). */
export type BoardEndpointTypeRow = {
  kind: LocalEndpointKind;
  surface: RoutePoolSurface;
  path: string;
  /** Created local entry keys for this type; not upstream logins. */
  keyCount: number;
};

export type BoardEndpointKeyTotals = {
  keys: number;
};

/** Four endpoint cards; counts are outbound entry keys already created. */
export function buildBoardEndpointTypeRows(
  createdKeyKinds: readonly LocalEndpointKind[] = [],
): BoardEndpointTypeRow[] {
  const counts: Record<LocalEndpointKind, number> = {
    messages: 0,
    responses_codex: 0,
    responses_grok: 0,
    chat_completions: 0,
  };
  for (const kind of createdKeyKinds) counts[kind] += 1;
  return LOCAL_ENDPOINT_KINDS.map((endpoint) => ({
    kind: endpoint.kind,
    surface: endpoint.surface,
    path: endpoint.path,
    keyCount: counts[endpoint.kind],
  }));
}

export function boardEndpointKeyTotals(
  rows: readonly BoardEndpointTypeRow[],
): BoardEndpointKeyTotals {
  return { keys: rows.reduce((sum, row) => sum + row.keyCount, 0) };
}

const STATE_RANK: Record<string, number> = {
  error: 0,
  degraded: 1,
  starting: 2,
  stopping: 3,
  running: 4,
  stopped: 5,
};

function summarizeRecent(
  status: AdapterBridgeRuntimeStatus | undefined,
): RouteBoardRecentSummary {
  const window = (status?.recentInbound ?? []).slice(0, BOARD_INBOUND_WINDOW);
  return {
    lastAt: status?.lastRequestAt?.trim() || window[0]?.at || null,
    failedInWindow: window.filter((row) => !row.ok).length,
    windowSize: window.length,
    totalRequestCount: status?.totalRequestCount ?? 0,
    failedRequestCount: status?.failedRequestCount ?? 0,
  };
}

export function boardAttentionReason(input: {
  statusUnavailable: boolean;
  state: AdapterBridgeRuntimeState | undefined;
  profileStatus: AdapterProfileStatus;
}): RouteBoardAttentionReason | null {
  if (input.statusUnavailable) return 'unavailable';
  if (input.state === 'error') return 'error';
  if (input.state === 'degraded') return 'degraded';
  if (input.profileStatus === 'needs_attention') return 'needs_attention';
  return null;
}

export function boardAttentionReasonLabel(
  reason: RouteBoardAttentionReason | null,
  lastErrorCode: string | null,
  t?: TranslateFn,
): string | null {
  if (!reason) return null;
  if (reason === 'unavailable') {
    return t ? t('routes.board.reasonUnavailable') : '状态不可用';
  }
  if (reason === 'degraded') {
    return t ? t('routes.board.reasonDegraded') : '上游异常或服务降级';
  }
  if (reason === 'needs_attention') {
    return t ? t('routes.board.reasonNeedsAttention') : '需要处理';
  }
  if (lastErrorCode) {
    return t
      ? t('routes.board.reasonErrorWithCode', { code: lastErrorCode })
      : `启动失败（${lastErrorCode}）`;
  }
  return t ? t('routes.board.reasonError') : '启动失败';
}

export function boardPoolLabel(
  pool: Pick<DefaultRoutePoolOverview, 'targetAgentId' | 'surface' | 'dialect'>,
  t?: TranslateFn,
): string {
  const kind = localEndpointKindFromPool(pool);
  const surfaceLabel = kind
    ? localEndpointKindLabel(kind, t)
    : pool.surface;
  return `${agentDisplayName(pool.targetAgentId)} · ${surfaceLabel}`;
}

/** Local listeners that belong to this connection-pool entry. */
export function profilesForPool<
  T extends Pick<AdapterProfile, 'id' | 'route' | 'sourceKind' | 'sourceId' | 'targetAgentId'>,
>(
  pool: Pick<DefaultRoutePoolOverview, 'id' | 'targetAgentId' | 'members'>,
  profiles: readonly T[],
): T[] {
  return profiles.filter((profile) => {
    if (profile.route !== 'local_bridge') return false;
    if (profile.id === pool.id) return true;
    if (profile.targetAgentId !== pool.targetAgentId) return false;
    return pool.members.some((member) => (
      member.sourceKind === profile.sourceKind && member.sourceId === profile.sourceId
    ));
  });
}

function pickRuntimeProfile(
  matches: readonly AdapterProfile[],
  statuses: Record<string, AdapterBridgeRuntimeStatus | undefined>,
): AdapterProfile | null {
  if (matches.length === 0) return null;
  return matches.find((profile) => {
    const state = statuses[profile.id]?.state;
    return state === 'running' || state === 'degraded';
  }) ?? matches[0] ?? null;
}

function statusRowFromRuntime(input: {
  id: string;
  name: string;
  targetAgentId: AgentId;
  memberCount: number;
  profile: AdapterProfile | null;
  portHint: number | null | undefined;
  status: AdapterBridgeRuntimeStatus | undefined;
  statusUnavailable: boolean;
}): RouteBoardStatusRow {
  const { profile, status, statusUnavailable } = input;
  const port = statusUnavailable ? null : (status?.port ?? input.portHint ?? profile?.localPort);
  const state = status?.state;
  const attentionReason = boardAttentionReason({
    statusUnavailable,
    state,
    profileStatus: profile?.status ?? 'active',
  });
  return {
    profileId: input.id,
    name: input.name,
    targetAgentId: input.targetAgentId,
    memberCount: input.memberCount,
    state,
    endpoint: typeof port === 'number' && port > 0 ? `127.0.0.1:${port}` : null,
    upstreamStatus: status?.upstreamStatus ?? null,
    lastErrorCode: profile?.lastErrorCode?.trim() || null,
    startedAt: status?.startedAt?.trim() || null,
    statusUnavailable,
    profileStatus: profile?.status ?? 'active',
    profile,
    recent: summarizeRecent(status),
    needsAttention: attentionReason != null,
    attentionReason,
  };
}

function sortBoardRows(rows: RouteBoardStatusRow[]): RouteBoardStatusRow[] {
  return rows.sort((a, b) => {
    if (a.needsAttention !== b.needsAttention) return a.needsAttention ? -1 : 1;
    const ra = a.statusUnavailable ? -1 : (STATE_RANK[a.state ?? 'stopped'] ?? 9);
    const rb = b.statusUnavailable ? -1 : (STATE_RANK[b.state ?? 'stopped'] ?? 9);
    if (ra !== rb) return ra - rb;
    return a.name.localeCompare(b.name);
  });
}

/** One card per connection-pool local entry, plus leftover listeners. */
export function buildRouteBoardStatusRows(
  profiles: readonly AdapterProfile[],
  bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus | undefined>,
  statusErrors: Record<string, unknown> = {},
  hiddenTargetIds: ReadonlySet<string> = new Set(),
  pools: readonly DefaultRoutePoolOverview[] = [],
  t?: TranslateFn,
): RouteBoardStatusRow[] {
  const covered = new Set<string>();
  const rows: RouteBoardStatusRow[] = [];
  for (const pool of pools) {
    if (hiddenTargetIds.has(pool.targetAgentId)) continue;
    if (pool.members.length === 0) continue;
    const matches = profilesForPool(pool, profiles);
    for (const match of matches) covered.add(match.id);
    const profile = pickRuntimeProfile(matches, bridgeStatuses);
    const statusId = profile?.id ?? pool.id;
    rows.push(statusRowFromRuntime({
      id: pool.id,
      name: boardPoolLabel(pool, t),
      targetAgentId: pool.targetAgentId,
      memberCount: pool.members.length,
      profile,
      portHint: pool.gatewayPort,
      status: bridgeStatuses[statusId],
      statusUnavailable: Boolean(statusErrors[statusId]),
    }));
  }
  for (const profile of profiles) {
    if (profile.route !== 'local_bridge') continue;
    if (hiddenTargetIds.has(profile.targetAgentId)) continue;
    if (covered.has(profile.id)) continue;
    const status = bridgeStatuses[profile.id];
    rows.push(statusRowFromRuntime({
      id: profile.id,
      name: profile.name.trim() || profile.targetAgentId,
      targetAgentId: profile.targetAgentId,
      memberCount: 0,
      profile,
      portHint: profile.localPort,
      status,
      statusUnavailable: Boolean(statusErrors[profile.id]),
    }));
  }
  return sortBoardRows(rows);
}

export function partitionBoardRows(rows: readonly RouteBoardStatusRow[]): {
  attention: RouteBoardStatusRow[];
  rest: RouteBoardStatusRow[];
} {
  const attention: RouteBoardStatusRow[] = [];
  const rest: RouteBoardStatusRow[] = [];
  for (const row of rows) {
    if (row.needsAttention) attention.push(row);
    else rest.push(row);
  }
  return { attention, rest };
}

/** Process-lifetime request counters summed across route rows (reset on app restart). */
export function sumRouteRequestTotals(rows: readonly RouteBoardStatusRow[]): {
  total: number;
  failed: number;
} {
  let total = 0;
  let failed = 0;
  for (const row of rows) {
    total += row.recent.totalRequestCount;
    failed += row.recent.failedRequestCount;
  }
  return { total, failed };
}

export function boardFleetSummary(
  rows: readonly RouteBoardStatusRow[],
  t?: TranslateFn,
): BoardFleetSummary | null {
  if (rows.length === 0) return null;
  const running = rows.filter((row) => {
    if (row.statusUnavailable) return false;
    return row.state === 'running' || row.state === 'degraded';
  }).length;
  const needsAttention = rows.filter((row) => row.needsAttention).length;
  const label = t
    ? needsAttention > 0
      ? t('routes.board.fleetSummaryAttention', {
          total: rows.length,
          running,
          attention: needsAttention,
        })
      : t('routes.fleetSummary', { total: rows.length, running })
    : needsAttention > 0
      ? `${rows.length} 个本机路由 · ${running} 个运行中 · ${needsAttention} 个需要处理`
      : `${rows.length} 个本机路由 · ${running} 个运行中 · 需保持托盘运行`;
  return { total: rows.length, running, needsAttention, label };
}

/** Newest-first merge of recentInbound across bridges, capped. */
export function mergeRecentInbound(
  profiles: readonly Pick<AdapterProfile, 'id' | 'name' | 'route' | 'targetAgentId'>[],
  bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus | undefined>,
  limit = BOARD_INBOUND_SNAPSHOT_LIMIT,
): MergedInboundRow[] {
  const rows: MergedInboundRow[] = [];
  for (const profile of profiles) {
    if (profile.route !== 'local_bridge') continue;
    const status = bridgeStatuses[profile.id];
    const label = profile.name.trim() || profile.targetAgentId;
    for (const row of status?.recentInbound ?? []) {
      rows.push({ ...row, profileId: profile.id, sourceLabel: label });
    }
  }
  rows.sort((a, b) => (a.at < b.at ? 1 : a.at > b.at ? -1 : 0));
  return rows.slice(0, limit);
}

export function countFailedInbound(rows: readonly { ok: boolean }[]): number {
  return rows.filter((row) => !row.ok).length;
}

export function activityHref(input: {
  filter?: 'all' | 'failed';
  route?: string | null;
}): string {
  const params = new URLSearchParams();
  if (input.filter && input.filter !== 'all') params.set('filter', input.filter);
  if (input.route) params.set('route', input.route);
  const qs = params.toString();
  return qs ? `/routes/activity?${qs}` : '/routes/activity';
}

export function parseActivityFilter(raw: string | null): 'all' | 'failed' {
  return raw === 'failed' ? 'failed' : 'all';
}

export function boardRecentSummaryLabel(
  recent: RouteBoardRecentSummary,
  relativeLast: string | null,
  t?: TranslateFn,
): string | null {
  if (recent.windowSize === 0 && recent.totalRequestCount === 0) {
    return t ? t('routes.board.recentNone') : '还没有请求';
  }
  if (recent.failedInWindow > 0) {
    return t
      ? t('routes.board.recentFailed', {
          relative: relativeLast ?? '—',
          failed: recent.failedInWindow,
          window: recent.windowSize,
        })
      : `${relativeLast ?? '—'} · 近 ${recent.windowSize} 条中 ${recent.failedInWindow} 条失败`;
  }
  if (recent.windowSize === 0) {
    return relativeLast
      ? (t ? t('routes.board.recentLastOnly', { relative: relativeLast }) : relativeLast)
      : null;
  }
  return t
    ? t('routes.board.recentOk', {
        relative: relativeLast ?? '—',
        window: recent.windowSize,
      })
    : `${relativeLast ?? '—'} · 近 ${recent.windowSize} 条`;
}

export function boardLifetimeSummaryLabel(
  recent: RouteBoardRecentSummary,
  t?: TranslateFn,
): string | null {
  if (recent.totalRequestCount <= 0) return null;
  if (recent.failedRequestCount > 0) {
    return t
      ? t('routes.board.lifetimeFailed', {
          total: recent.totalRequestCount,
          failed: recent.failedRequestCount,
        })
      : `共 ${recent.totalRequestCount} 次 · 失败 ${recent.failedRequestCount} 次`;
  }
  return t
    ? t('routes.board.lifetimeOk', { total: recent.totalRequestCount })
    : `共 ${recent.totalRequestCount} 次`;
}
