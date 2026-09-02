/**
 * Pure helpers for the Routes activity feed.
 */
import type {
  AdapterBridgeInboundRequest,
  AdapterBridgeRuntimeStatus,
  AdapterProfile,
} from '@/lib/backend/contracts/adapter';
import {
  mergeRecentInbound,
  parseActivityFilter,
  type MergedInboundRow,
} from '../board/board-view-model';

export type InboundFeedFilter = 'all' | 'failed';

export function filterInboundFeed(
  rows: readonly MergedInboundRow[],
  filter: InboundFeedFilter,
  routeId?: string | null,
): MergedInboundRow[] {
  let next = filter === 'failed' ? rows.filter((row) => !row.ok) : [...rows];
  if (routeId) next = next.filter((row) => row.profileId === routeId);
  return next;
}

export function buildInboundFeed(
  profiles: readonly Pick<AdapterProfile, 'id' | 'name' | 'route' | 'targetAgentId'>[],
  bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus | undefined>,
  filter: InboundFeedFilter,
  limit = 20,
  routeId?: string | null,
): MergedInboundRow[] {
  return filterInboundFeed(
    mergeRecentInbound(profiles, bridgeStatuses, limit * 4),
    filter,
    routeId,
  ).slice(0, limit);
}

export function countFailedInbound(rows: readonly AdapterBridgeInboundRequest[]): number {
  return rows.filter((row) => !row.ok).length;
}

export { parseActivityFilter };

export type ActivityRouteOption = {
  id: string;
  label: string;
};

/** Distinct monitored routes for the activity page filter. */
export function activityRouteOptions(
  profiles: readonly Pick<AdapterProfile, 'id' | 'name' | 'route' | 'targetAgentId'>[],
): ActivityRouteOption[] {
  return profiles
    .map((profile) => ({
      id: profile.id,
      label: profile.name.trim() || profile.targetAgentId,
    }))
    .sort((a, b) => a.label.localeCompare(b.label));
}
