import type {
  AdapterBridgeInboundRequest,
  AdapterBridgeRuntimeState,
  AdapterBridgeRuntimeStatus,
  AdapterProfile,
} from '@/lib/backend/contracts/adapter';

export type RouteBoardStatusRow = {
  profileId: string;
  name: string;
  state: AdapterBridgeRuntimeState | undefined;
  endpoint: string | null;
  upstreamStatus: AdapterBridgeRuntimeStatus['upstreamStatus'];
  lastErrorCode: string | null;
};

export type MergedInboundRow = AdapterBridgeInboundRequest & {
  profileId: string;
  sourceLabel: string;
};

const STATE_RANK: Record<string, number> = {
  error: 0,
  degraded: 1,
  starting: 2,
  stopping: 3,
  running: 4,
  stopped: 5,
};

/** Local-bridge rows for the board, error/degraded first. */
export function buildRouteBoardStatusRows(
  profiles: readonly AdapterProfile[],
  bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus | undefined>,
): RouteBoardStatusRow[] {
  const bridges = profiles.filter((profile) => profile.route === 'local_bridge');
  const rows = bridges.map((profile) => {
    const status = bridgeStatuses[profile.id];
    const port = status?.port ?? profile.localPort;
    const endpoint =
      typeof port === 'number' && port > 0 ? `127.0.0.1:${port}` : null;
    return {
      profileId: profile.id,
      name: profile.name.trim() || profile.targetAgentId,
      state: status?.state,
      endpoint,
      upstreamStatus: status?.upstreamStatus ?? null,
      lastErrorCode: profile.lastErrorCode?.trim() || null,
    };
  });
  return rows.sort((a, b) => {
    const ra = STATE_RANK[a.state ?? 'stopped'] ?? 9;
    const rb = STATE_RANK[b.state ?? 'stopped'] ?? 9;
    if (ra !== rb) return ra - rb;
    return a.name.localeCompare(b.name);
  });
}

/** Newest-first merge of recentInbound across bridges, capped. */
export function mergeRecentInbound(
  profiles: readonly Pick<AdapterProfile, 'id' | 'name' | 'route' | 'targetAgentId'>[],
  bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus | undefined>,
  limit = 20,
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
