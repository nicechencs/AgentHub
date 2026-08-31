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
  /** Safe default display; the raw token remains available only for copy/reveal. */
  maskedToken: string | null;
  /** The runtime status read failed, so no token action is safe. */
  unavailable: boolean;
}

export function maskLocalToken(token: string): string {
  const trimmed = token.trim();
  if (!trimmed) return '';
  const tail = trimmed.slice(-4);
  return trimmed.startsWith('ahb_') ? `ahb_••••${tail}` : `••••${tail}`;
}

/** One row per local-bridge route (per upstream source, same grouping as the board). */
export function buildLocalTokenRows(
  profiles: readonly AdapterProfile[],
  bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus | undefined>,
  statusErrors: Readonly<Record<string, unknown>> = {},
): LocalTokenRow[] {
  const rows: LocalTokenRow[] = [];
  for (const profile of profiles) {
    if (profile.route !== 'local_bridge') continue;
    const status = bridgeStatuses[profile.id];
    const unavailable = Boolean(statusErrors[profile.id]) || status?.upstreamStatus === 'unavailable';
    const port = status?.port ?? profile.localPort;
    const token = unavailable ? null : status?.localToken?.trim() || null;
    rows.push({
      profileId: profile.id,
      name: profile.name.trim() || profile.targetAgentId,
      endpoint: typeof port === 'number' && port > 0 ? `127.0.0.1:${port}` : null,
      state: status?.state,
      token,
      maskedToken: token ? maskLocalToken(token) : null,
      unavailable,
    });
  }
  return rows.sort((a, b) => a.name.localeCompare(b.name));
}
