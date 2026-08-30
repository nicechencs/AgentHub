import type {
  AdapterBridgeInboundRequest,
  AdapterBridgeRuntimeStatus,
  AdapterProfile,
} from '@/lib/backend/contracts/adapter';
import { mergeRecentInbound, type MergedInboundRow } from '../board/board-view-model';

export type InboundFeedFilter = 'all' | 'failed';

export function filterInboundFeed(
  rows: readonly MergedInboundRow[],
  filter: InboundFeedFilter,
): MergedInboundRow[] {
  if (filter === 'failed') return rows.filter((row) => !row.ok);
  return [...rows];
}

export function buildInboundFeed(
  profiles: readonly Pick<AdapterProfile, 'id' | 'name' | 'route' | 'targetAgentId'>[],
  bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus | undefined>,
  filter: InboundFeedFilter,
  limit = 20,
): MergedInboundRow[] {
  return filterInboundFeed(mergeRecentInbound(profiles, bridgeStatuses, limit * 4), filter).slice(
    0,
    limit,
  );
}

export function countFailedInbound(rows: readonly AdapterBridgeInboundRequest[]): number {
  return rows.filter((row) => !row.ok).length;
}
