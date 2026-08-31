/**
 * Pure view-model for Routes board. No React, no IO.
 */
import type {
  AdapterBridgeInboundRequest,
  AdapterBridgeRuntimeState,
  AdapterBridgeRuntimeStatus,
  AdapterProfile,
  AdapterProfileStatus,
} from '@/lib/backend/contracts/adapter';
import type { TranslateFn } from '@/lib/i18n';

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
  state: AdapterBridgeRuntimeState | undefined;
  endpoint: string | null;
  upstreamStatus: AdapterBridgeRuntimeStatus['upstreamStatus'];
  lastErrorCode: string | null;
  startedAt: string | null;
  statusUnavailable: boolean;
  profileStatus: AdapterProfileStatus;
  /** Full profile for start/stop mutations (group semantics use source). */
  profile: AdapterProfile;
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

/** One card per local listener (same unit as default-pool / 本机入口). */
export function buildRouteBoardStatusRows(
  profiles: readonly AdapterProfile[],
  bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus | undefined>,
  statusErrors: Record<string, unknown> = {},
  hiddenTargetIds: ReadonlySet<string> = new Set(),
): RouteBoardStatusRow[] {
  const local = profiles.filter((profile) => {
    if (profile.route !== 'local_bridge') return false;
    if (hiddenTargetIds.has(profile.targetAgentId)) return false;
    return true;
  });
  const rows = local.map((profile) => {
    const status = bridgeStatuses[profile.id];
    const statusUnavailable = Boolean(statusErrors[profile.id]);
    const port = statusUnavailable ? null : (status?.port ?? profile.localPort);
    const endpoint =
      typeof port === 'number' && port > 0 ? `127.0.0.1:${port}` : null;
    const state = status?.state;
    const attentionReason = boardAttentionReason({
      statusUnavailable,
      state,
      profileStatus: profile.status,
    });
    return {
      profileId: profile.id,
      name: profile.name.trim() || profile.targetAgentId,
      state,
      endpoint,
      upstreamStatus: status?.upstreamStatus ?? null,
      lastErrorCode: profile.lastErrorCode?.trim() || null,
      startedAt: status?.startedAt?.trim() || null,
      statusUnavailable,
      profileStatus: profile.status,
      profile,
      recent: summarizeRecent(status),
      needsAttention: attentionReason != null,
      attentionReason,
    };
  });
  return rows.sort((a, b) => {
    if (a.needsAttention !== b.needsAttention) return a.needsAttention ? -1 : 1;
    const ra = a.statusUnavailable ? -1 : (STATE_RANK[a.state ?? 'stopped'] ?? 9);
    const rb = b.statusUnavailable ? -1 : (STATE_RANK[b.state ?? 'stopped'] ?? 9);
    if (ra !== rb) return ra - rb;
    return a.name.localeCompare(b.name);
  });
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
