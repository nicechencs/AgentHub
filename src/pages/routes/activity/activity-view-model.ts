/**
 * Activity (monitoring) page state and trace feed assembly.
 */
import type {
  AdapterBridgeRouteTrace,
  AdapterBridgeRuntimeStatus,
  AdapterProfile,
  DefaultRoutePoolOverview,
} from '@/lib/backend/contracts/adapter';
import { isBridgeStopCapable } from '@/pages/bridges/adapter-view-model';
import { buildLocalEntryControl } from '@/pages/routes/board/board-view-model';
import {
  buildRouteTraceFeed,
  type MergedRouteTraceRow,
} from './route-trace-feed-model';
import { parseActivityFilter, type InboundFeedFilter } from './inbound-feed-model';

export type ActivityPageKind =
  | 'loading'
  | 'error'
  | 'noLogins'
  | 'noRoutes'
  | 'notRunning'
  | 'runningEmpty'
  | 'filteredEmpty'
  | 'ready';

export type ActivityPageSnapshot = {
  kind: ActivityPageKind;
  feed: MergedRouteTraceRow[];
  monitoredProfileIds: string[];
  runningCount: number;
  hasEnrolledLogins: boolean;
  allCount: number;
  failedCount: number;
};

export function monitoredLocalProfiles(
  profiles: readonly Pick<AdapterProfile, 'id' | 'name' | 'route' | 'targetAgentId' | 'sourceKind' | 'sourceId' | 'lastErrorCode'>[],
  hiddenTargetIds: ReadonlySet<string> = new Set(),
  pools: readonly Pick<DefaultRoutePoolOverview, 'id' | 'targetAgentId' | 'members'>[] = [],
): Pick<AdapterProfile, 'id' | 'name' | 'route' | 'targetAgentId'>[] {
  const control = buildLocalEntryControl(profiles, {}, hiddenTargetIds, pools);
  const byId = new Map(profiles.map((profile) => [profile.id, profile]));
  return control.profileIds
    .map((id) => byId.get(id))
    .filter((profile): profile is AdapterProfile => profile != null)
    .map((profile) => ({
      id: profile.id,
      name: profile.name,
      route: profile.route,
      targetAgentId: profile.targetAgentId,
    }));
}

function runningProfileCount(
  profileIds: readonly string[],
  bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus | undefined>,
): number {
  return profileIds.filter((id) => isBridgeStopCapable(bridgeStatuses[id]?.state)).length;
}

function mergeBridgeStatuses(
  primary: Record<string, AdapterBridgeRuntimeStatus | undefined>,
  extra: readonly AdapterBridgeRuntimeStatus[] | undefined,
): Record<string, AdapterBridgeRuntimeStatus | undefined> {
  if (!extra?.length) return primary;
  const merged = { ...primary };
  for (const status of extra) {
    merged[status.profileId] = status;
  }
  return merged;
}

export function resolveActivityPageSnapshot(input: {
  profiles: readonly AdapterProfile[];
  bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus | undefined>;
  localEntryStatuses?: readonly AdapterBridgeRuntimeStatus[];
  unauthenticatedTraces?: readonly AdapterBridgeRouteTrace[];
  unauthenticatedSourceLabel?: string;
  pools?: readonly DefaultRoutePoolOverview[];
  hiddenTargetIds?: ReadonlySet<string>;
  filter: InboundFeedFilter;
  routeId?: string | null;
  profileState: 'loading' | 'ready' | 'error';
  loading: boolean;
}): ActivityPageSnapshot {
  const pools = input.pools ?? [];
  const monitored = monitoredLocalProfiles(input.profiles, input.hiddenTargetIds, pools);
  const control = buildLocalEntryControl(
    input.profiles,
    input.bridgeStatuses,
    input.hiddenTargetIds,
    pools,
  );
  const statuses = mergeBridgeStatuses(
    input.bridgeStatuses,
    input.localEntryStatuses,
  );
  const traceExtras = {
    unauthenticatedTraces: input.unauthenticatedTraces,
    unauthenticatedSourceLabel: input.unauthenticatedSourceLabel,
  };
  const feed = buildRouteTraceFeed(monitored, statuses, input.filter, 20, input.routeId, traceExtras);
  const allCount = buildRouteTraceFeed(monitored, statuses, 'all', 100, input.routeId, traceExtras).length;
  const failedCount = buildRouteTraceFeed(monitored, statuses, 'failed', 100, input.routeId, traceExtras).length;
  const runningCount = runningProfileCount(control.profileIds, statuses);
  const hasEnrolledLogins = control.hasEnrolledLogins;
  const filteredEmpty = Boolean(input.routeId || input.filter === 'failed');

  if (input.profileState === 'error') {
    return {
      kind: 'error',
      feed,
      monitoredProfileIds: monitored.map((profile) => profile.id),
      runningCount,
      hasEnrolledLogins,
      allCount,
      failedCount,
    };
  }
  if (input.loading || input.profileState === 'loading') {
    return {
      kind: 'loading',
      feed,
      monitoredProfileIds: monitored.map((profile) => profile.id),
      runningCount,
      hasEnrolledLogins,
      allCount,
      failedCount,
    };
  }
  if (!hasEnrolledLogins && monitored.length === 0) {
    return {
      kind: 'noLogins',
      feed,
      monitoredProfileIds: [],
      runningCount,
      hasEnrolledLogins,
      allCount,
      failedCount,
    };
  }
  if (monitored.length === 0) {
    return {
      kind: 'noRoutes',
      feed,
      monitoredProfileIds: [],
      runningCount,
      hasEnrolledLogins,
      allCount,
      failedCount,
    };
  }
  if (runningCount === 0) {
    return {
      kind: 'notRunning',
      feed,
      monitoredProfileIds: monitored.map((profile) => profile.id),
      runningCount,
      hasEnrolledLogins,
      allCount,
      failedCount,
    };
  }
  if (feed.length === 0 && filteredEmpty) {
    return {
      kind: 'filteredEmpty',
      feed,
      monitoredProfileIds: monitored.map((profile) => profile.id),
      runningCount,
      hasEnrolledLogins,
      allCount,
      failedCount,
    };
  }
  if (feed.length === 0) {
    return {
      kind: 'runningEmpty',
      feed,
      monitoredProfileIds: monitored.map((profile) => profile.id),
      runningCount,
      hasEnrolledLogins,
      allCount,
      failedCount,
    };
  }
  return {
    kind: 'ready',
    feed,
    monitoredProfileIds: monitored.map((profile) => profile.id),
    runningCount,
    hasEnrolledLogins,
    allCount,
    failedCount,
  };
}

export { parseActivityFilter };
