/**
 * Pure helpers for the Routes activity feed (route request traces).
 */
import type {
  AdapterBridgeInboundRequest,
  AdapterBridgeRouteTrace,
  AdapterBridgeRuntimeStatus,
  AdapterProfile,
  RouteTraceStageStatus,
} from '@/lib/backend/contracts/adapter';
import { parseActivityFilter, type InboundFeedFilter } from './inbound-feed-model';

export const UNAUTHENTICATED_TRACE_PROFILE_ID = '__unauthenticated__';

export type MergedRouteTraceRow = AdapterBridgeRouteTrace & {
  profileId: string;
  sourceLabel: string;
  /** Synthesized from legacy recentInbound when the backend has no traces yet. */
  legacySummary?: boolean;
  /** Local-auth failure without a bound route. */
  unauthenticated?: boolean;
};

export type RouteTraceFeedExtras = {
  unauthenticatedTraces?: readonly AdapterBridgeRouteTrace[];
  unauthenticatedSourceLabel?: string;
};

const skippedStage = { status: 'skipped' as RouteTraceStageStatus };

function inboundToLegacyTrace(
  row: AdapterBridgeInboundRequest,
  index: number,
): AdapterBridgeRouteTrace {
  return {
    requestId: `legacy-${row.at}-${row.method}-${row.path}-${index}`,
    at: row.at,
    method: row.method,
    path: row.path,
    httpStatus: row.status,
    ok: row.ok,
    localAuth: skippedStage,
    pool: skippedStage,
    conversion: { ...skippedStage, path: '' },
    upstreamAuth: skippedStage,
    upstream: skippedStage,
    failureStage: row.ok ? null : 'upstream',
  };
}

export function mergeRecentRouteTraces(
  profiles: readonly Pick<AdapterProfile, 'id' | 'name' | 'route' | 'targetAgentId'>[],
  bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus | undefined>,
  limit = 60,
  extras: RouteTraceFeedExtras = {},
): MergedRouteTraceRow[] {
  const labelById = new Map(
    profiles.map((profile) => [profile.id, profile.name.trim() || profile.targetAgentId]),
  );
  const profileIds = new Set(profiles.map((profile) => profile.id));
  for (const profileId of Object.keys(bridgeStatuses)) {
    if (bridgeStatuses[profileId]) profileIds.add(profileId);
  }
  const rows: MergedRouteTraceRow[] = [];
  for (const profileId of profileIds) {
    const status = bridgeStatuses[profileId];
    if (!status) continue;
    const label = labelById.get(profileId) ?? profileId;
    const traces = status.recentRouteTraces ?? [];
    if (traces.length > 0) {
      for (const row of traces) {
        rows.push({ ...row, profileId, sourceLabel: label });
      }
      continue;
    }
    for (const [index, row] of (status.recentInbound ?? []).entries()) {
      rows.push({
        ...inboundToLegacyTrace(row, index),
        profileId,
        sourceLabel: label,
        legacySummary: true,
      });
    }
  }
  const unauthenticatedLabel = extras.unauthenticatedSourceLabel?.trim()
    || UNAUTHENTICATED_TRACE_PROFILE_ID;
  for (const row of extras.unauthenticatedTraces ?? []) {
    rows.push({
      ...row,
      profileId: UNAUTHENTICATED_TRACE_PROFILE_ID,
      sourceLabel: unauthenticatedLabel,
      unauthenticated: true,
    });
  }
  rows.sort((a, b) => (a.at < b.at ? 1 : a.at > b.at ? -1 : 0));
  return rows.slice(0, limit);
}

export function filterRouteTraceFeed(
  rows: readonly MergedRouteTraceRow[],
  filter: InboundFeedFilter,
  routeId?: string | null,
): MergedRouteTraceRow[] {
  let next = filter === 'failed' ? rows.filter((row) => !row.ok) : [...rows];
  if (routeId) next = next.filter((row) => row.profileId === routeId);
  return next;
}

export function buildRouteTraceFeed(
  profiles: readonly Pick<AdapterProfile, 'id' | 'name' | 'route' | 'targetAgentId'>[],
  bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus | undefined>,
  filter: InboundFeedFilter,
  limit = 20,
  routeId?: string | null,
  extras: RouteTraceFeedExtras = {},
): MergedRouteTraceRow[] {
  return filterRouteTraceFeed(
    mergeRecentRouteTraces(profiles, bridgeStatuses, limit * 4, extras),
    filter,
    routeId,
  ).slice(0, limit);
}

export { parseActivityFilter };
