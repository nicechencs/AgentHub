/**
 * Pure view-model for board usage stats. No React, no IO.
 *
 * Charts use local-gateway request rows (what went through a local route),
 * not Agent session logs. Layout mirrors 总览: overlay by endpoint type when
 * all types are selected, distribution by model after picking one type.
 */
import type {
  AdapterProfile,
  DefaultRoutePoolOverview,
  RoutePoolSurface,
} from '@/lib/backend/contracts/adapter';
import type { GatewayUsageRow } from '@/lib/backend/contracts/usage-types';
import type { UsageTrendPoint } from '@/lib/types';
import { denseTrendBuckets, localTrendBucket, trendGrain } from '@/lib/usage-trend';
import { boardPoolLabel, profilesForPool } from '@/pages/routes/board/board-view-model';

export type BoardUsageRange = 'today' | '24h' | '7d' | '30d';
export type BoardGroupBy = 'entry' | 'model' | 'surface';
/** Wire values from gateway capture (`DownstreamSurface::op`). */
export type BoardUsageSurface = 'messages' | 'responses' | 'chat';

export const BOARD_SURFACES: readonly BoardUsageSurface[] = [
  'messages',
  'responses',
  'chat',
];

export interface BoardUsageFilters {
  dateRange: BoardUsageRange;
  entryId: string;
  surface: string;
  modelFilter: string;
}

export const DEFAULT_BOARD_USAGE_FILTERS: BoardUsageFilters = {
  dateRange: '7d',
  entryId: 'all',
  surface: 'all',
  modelFilter: 'all',
};

/**
 * Same rule as 总览: all endpoint types → by type; one type or one local
 * entry → by model.
 */
export function deriveBoardGroupBy(entryId: string, surface = 'all'): BoardGroupBy {
  if (entryId !== 'all' && entryId !== '') return 'model';
  if (surface !== 'all' && surface !== '') return 'model';
  return 'surface';
}

/** Pool surface (`chat_completions`) → gateway capture op (`chat`). */
export function poolSurfaceToUsageSurface(
  surface: RoutePoolSurface | 'all',
): string {
  if (surface === 'all') return 'all';
  if (surface === 'chat_completions') return 'chat';
  return surface;
}

export function usageSurfaceToPoolSurface(
  surface: string,
): RoutePoolSurface | 'all' {
  if (surface === 'chat' || surface === 'chat_completions') return 'chat_completions';
  if (surface === 'messages' || surface === 'responses') return surface;
  return 'all';
}

let rememberedFilters: BoardUsageFilters = { ...DEFAULT_BOARD_USAGE_FILTERS };

export function rememberedBoardUsageFilters(): BoardUsageFilters {
  return { ...rememberedFilters };
}

export function rememberBoardUsageFilters(next: BoardUsageFilters): void {
  rememberedFilters = { ...next };
}

export function boardUsageWindow(
  range: BoardUsageRange,
  now = new Date(),
): { days: number; since: string } {
  if (range === 'today') {
    const midnight = new Date(now.getFullYear(), now.getMonth(), now.getDate());
    return { days: 1, since: midnight.toISOString() };
  }
  const days = range === '24h' ? 1 : range === '7d' ? 7 : 30;
  return { days, since: new Date(now.getTime() - days * 24 * 3600 * 1000).toISOString() };
}

export interface BoardUsageEntry {
  id: string;
  name: string;
  profileIds: string[];
  targetAgentId: string;
}

export function buildBoardUsageEntries(
  profiles: readonly Pick<
    AdapterProfile,
    'id' | 'name' | 'route' | 'sourceKind' | 'sourceId' | 'targetAgentId'
  >[],
  hiddenTargetIds: ReadonlySet<string> = new Set(),
  pools: readonly DefaultRoutePoolOverview[] = [],
): BoardUsageEntry[] {
  const entries: BoardUsageEntry[] = [];
  const covered = new Set<string>();
  for (const pool of pools) {
    if (hiddenTargetIds.has(pool.targetAgentId)) continue;
    if (pool.members.length === 0) continue;
    const matches = profilesForPool(pool, profiles);
    const profileIds = [...new Set([pool.id, ...matches.map((item) => item.id)])];
    for (const id of profileIds) covered.add(id);
    entries.push({
      id: pool.id,
      name: boardPoolLabel(pool),
      profileIds,
      targetAgentId: pool.targetAgentId,
    });
  }
  for (const profile of profiles) {
    if (profile.route !== 'local_bridge') continue;
    if (hiddenTargetIds.has(profile.targetAgentId)) continue;
    if (covered.has(profile.id)) continue;
    entries.push({
      id: profile.id,
      name: profile.name.trim() || profile.targetAgentId,
      profileIds: [profile.id],
      targetAgentId: profile.targetAgentId,
    });
  }
  return entries.sort((a, b) => a.name.localeCompare(b.name) || a.id.localeCompare(b.id));
}

export function gatewayRowTokens(row: Pick<
  GatewayUsageRow,
  'inputTokens' | 'outputTokens' | 'cachedInputTokens' | 'reasoningTokens'
>): number {
  return (
    Math.max(0, row.inputTokens) +
    Math.max(0, row.outputTokens) +
    Math.max(0, row.cachedInputTokens ?? 0) +
    Math.max(0, row.reasoningTokens ?? 0)
  );
}

export function filterGatewayUsageRows(
  rows: readonly GatewayUsageRow[],
  input: {
    profileIds?: readonly string[] | null;
    surface?: string;
    model?: string;
  },
): GatewayUsageRow[] {
  const profileSet =
    input.profileIds && input.profileIds.length > 0 ? new Set(input.profileIds) : null;
  const surface = !input.surface || input.surface === 'all' ? null : input.surface;
  const model = !input.model || input.model === 'all' ? null : input.model;
  return rows.filter((row) => {
    if (profileSet && !profileSet.has(row.profileId)) return false;
    if (surface && row.surface !== surface) return false;
    if (model && (row.model ?? '') !== model) return false;
    return true;
  });
}

export interface BoardUsageTotals {
  requestCount: number;
  okCount: number;
  failedCount: number;
  inputTokens: number;
  outputTokens: number;
  cachedInputTokens: number;
  totalTokens: number;
  avgLatencyMs: number | null;
  modelNames: string[];
}

export function summarizeGatewayUsage(rows: readonly GatewayUsageRow[]): BoardUsageTotals {
  let okCount = 0;
  let failedCount = 0;
  let inputTokens = 0;
  let outputTokens = 0;
  let cachedInputTokens = 0;
  let latencySum = 0;
  let latencyN = 0;
  const models = new Set<string>();
  for (const row of rows) {
    if (row.status === 'ok') okCount += 1;
    else failedCount += 1;
    inputTokens += Math.max(0, row.inputTokens);
    outputTokens += Math.max(0, row.outputTokens);
    cachedInputTokens += Math.max(0, row.cachedInputTokens ?? 0);
    if (typeof row.latencyMs === 'number') {
      latencySum += row.latencyMs;
      latencyN += 1;
    }
    if (row.model) models.add(row.model);
  }
  return {
    requestCount: rows.length,
    okCount,
    failedCount,
    inputTokens,
    outputTokens,
    cachedInputTokens,
    totalTokens: inputTokens + outputTokens + cachedInputTokens,
    avgLatencyMs: latencyN > 0 ? latencySum / latencyN : null,
    modelNames: [...models].sort((a, b) => a.localeCompare(b)),
  };
}

export interface BoardTrendSeries {
  key: string;
  label: string;
  color: string;
}

export interface BoardDistributionSlice {
  key: string;
  label: string;
  color: string;
  tokens: number;
  requests: number;
}

export function seriesKeyForRow(
  row: GatewayUsageRow,
  groupBy: BoardGroupBy,
  profileToEntry: ReadonlyMap<string, string>,
): string | null {
  if (groupBy === 'surface') return row.surface || null;
  if (groupBy === 'model') return row.model?.trim() || null;
  return profileToEntry.get(row.profileId) ?? row.profileId;
}

export function buildGatewayTrend(
  rows: readonly GatewayUsageRow[],
  days: number,
  since: string | undefined,
  seriesKeys: readonly string[],
  keyOf: (row: GatewayUsageRow) => string | null,
  now = new Date(),
): UsageTrendPoint[] {
  const grain = trendGrain(days);
  const keys = denseTrendBuckets(days, since, now);
  const byDate = new Map<string, UsageTrendPoint>();
  for (const key of keys) {
    const point: UsageTrendPoint = { date: key };
    for (const series of seriesKeys) point[series] = 0;
    byDate.set(key, point);
  }
  for (const row of rows) {
    const series = keyOf(row);
    if (!series || !seriesKeys.includes(series)) continue;
    const bucket = localTrendBucket(row.ts, grain);
    if (!bucket) continue;
    const point = byDate.get(bucket);
    if (!point) continue;
    point[series] = (point[series] as number) + gatewayRowTokens(row);
  }
  return [...byDate.values()].sort((a, b) => (a.date < b.date ? -1 : a.date > b.date ? 1 : 0));
}

export function buildGatewayDistribution(
  rows: readonly GatewayUsageRow[],
  groupBy: BoardGroupBy,
  profileToEntry: ReadonlyMap<string, string>,
  labels: Readonly<Record<string, { label: string; color: string }>>,
  fallbackColor = 'var(--text-muted)',
): BoardDistributionSlice[] {
  const byKey = new Map<string, BoardDistributionSlice>();
  for (const row of rows) {
    const key = seriesKeyForRow(row, groupBy, profileToEntry);
    if (!key) continue;
    const meta = labels[key];
    const entry = byKey.get(key) ?? {
      key,
      label: meta?.label ?? key,
      color: meta?.color ?? fallbackColor,
      tokens: 0,
      requests: 0,
    };
    entry.tokens += gatewayRowTokens(row);
    entry.requests += 1;
    byKey.set(key, entry);
  }
  return [...byKey.values()].sort((a, b) => b.tokens - a.tokens);
}

export function profileToEntryIdMap(
  entries: readonly BoardUsageEntry[],
): Map<string, string> {
  const map = new Map<string, string>();
  for (const entry of entries) {
    for (const profileId of entry.profileIds) map.set(profileId, entry.id);
  }
  return map;
}
