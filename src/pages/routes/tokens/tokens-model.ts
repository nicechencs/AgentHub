/**
 * Pure view-model for the local-tokens page. No React, no IO.
 */
import type {
  AdapterBridgeRuntimeState,
  AdapterBridgeRuntimeStatus,
  AdapterProfile,
} from '@/lib/backend/contracts/adapter';

export interface LocalTokenRow {
  profileId: string;
  name: string;
  endpoint: string | null;
  state: AdapterBridgeRuntimeState | undefined;
  /** Loopback bearer (`ahb_…`); null until the listener has started. */
  token: string | null;
}

/** One row per local-bridge route (per upstream source, same grouping as the board). */
export function buildLocalTokenRows(
  profiles: readonly AdapterProfile[],
  bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus | undefined>,
): LocalTokenRow[] {
  const rows: LocalTokenRow[] = [];
  for (const profile of profiles) {
    if (profile.route !== 'local_bridge') continue;
    const status = bridgeStatuses[profile.id];
    const port = status?.port ?? profile.localPort;
    rows.push({
      profileId: profile.id,
      name: profile.name.trim() || profile.targetAgentId,
      endpoint: typeof port === 'number' && port > 0 ? `127.0.0.1:${port}` : null,
      state: status?.state,
      token: status?.localToken?.trim() || null,
    });
  }
  return rows.sort((a, b) => a.name.localeCompare(b.name));
}
