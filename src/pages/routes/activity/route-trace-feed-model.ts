/**
 * Pure helpers for the Routes activity feed (route request traces).
 */
import type {
  AdapterBridgeRouteTrace,
  AdapterBridgeRuntimeStatus,
  AdapterProfile,
} from '@/lib/backend/contracts/adapter';
import { parseActivityFilter, type InboundFeedFilter } from './inbound-feed-model';

export type MergedRouteTraceRow = AdapterBridgeRouteTrace & {
  profileId: string;
  sourceLabel: string;
};

export function mergeRecentRouteTraces(
  profiles: readonly Pick<AdapterProfile, 'id' | 'name' | 'route' | 'targetAgentId'>[],
  bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus | undefined>,
  limit = 60,
): MergedRouteTraceRow[] {
  const rows: MergedRouteTraceRow[] = [];
  for (const profile of profiles) {
    if (profile.route !== 'local_bridge') continue;
    const status = bridgeStatuses[profile.id];
    const label = profile.name.trim() || profile.targetAgentId;
    for (const row of status?.recentRouteTraces ?? []) {
      rows.push({ ...row, profileId: profile.id, sourceLabel: label });
    }
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
): MergedRouteTraceRow[] {
  return filterRouteTraceFeed(
    mergeRecentRouteTraces(profiles, bridgeStatuses, limit * 4),
    filter,
    routeId,
  ).slice(0, limit);
}

export { parseActivityFilter };
