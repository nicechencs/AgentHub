/**
 * Pure view-model for the local-tokens page. No React, no IO.
 */
import type {
  AdapterBridgeRuntimeState,
  AdapterBridgeRuntimeStatus,
  AdapterProfile,
  DefaultRoutePoolOverview,
} from '@/lib/backend/contracts/adapter';
import type { TranslateFn } from '@/lib/i18n';
import {
  localEndpointKindForTargetAgent,
  localEndpointKindFromPool,
  localEndpointPath,
  type LocalEndpointKind,
} from '@/lib/route-endpoints';
import { localEndpointKindLabel } from '@/pages/bridges/route-pool-view-model';
import { profilesForPool } from '@/pages/routes/board/board-view-model';

export interface LocalTokenRow {
  /** Stable list key: pool id or profile id. */
  id: string;
  profileId: string | null;
  name: string;
  kind: LocalEndpointKind;
  path: string;
  endpoint: string | null;
  state: AdapterBridgeRuntimeState | undefined;
  /** Loopback bearer (`ahb_…`); null until the listener has started. */
  token: string | null;
  /** Safe default display; the raw token remains available only for copy/reveal. */
  maskedToken: string | null;
  /** The runtime status read failed, so no token action is safe. */
  unavailable: boolean;
  /** Pool/runtime writer this key belongs to; not a client-write target. */
  targetAgentId: string;
}

export function maskLocalToken(token: string): string {
  const trimmed = token.trim();
  if (!trimmed) return '';
  const tail = trimmed.slice(-4);
  return trimmed.startsWith('ahb_') ? `ahb_••••${tail}` : `••••${tail}`;
}

export function tokenListenPort(endpoint: string | null): number | null {
  if (!endpoint) return null;
  const colon = endpoint.lastIndexOf(':');
  if (colon < 0) return null;
  const port = Number(endpoint.slice(colon + 1));
  return Number.isInteger(port) && port > 0 ? port : null;
}

/** Token type shown on the list and in details. Not a writer Agent. */
export function tokenTypeLabel(
  row: Pick<LocalTokenRow, 'kind'>,
  t?: TranslateFn,
): string {
  return localEndpointKindLabel(row.kind, t);
}

function pickRuntimeProfile(
  matches: readonly AdapterProfile[],
  statuses: Record<string, AdapterBridgeRuntimeStatus | undefined>,
): AdapterProfile | null {
  if (matches.length === 0) return null;
  return matches.find((profile) => {
    const state = statuses[profile.id]?.state;
    return state === 'running' || state === 'degraded';
  }) ?? matches[0] ?? null;
}

function rowFromRuntime(input: {
  id: string;
  name: string;
  kind: LocalEndpointKind;
  targetAgentId: string;
  profile: AdapterProfile | null;
  portHint: number | null | undefined;
  status: AdapterBridgeRuntimeStatus | undefined;
  unavailable: boolean;
}): LocalTokenRow {
  const port = input.unavailable
    ? null
    : (input.status?.port ?? input.portHint ?? input.profile?.localPort);
  const token = input.unavailable ? null : input.status?.localToken?.trim() || null;
  return {
    id: input.id,
    profileId: input.profile?.id ?? null,
    name: input.name,
    kind: input.kind,
    path: localEndpointPath(input.kind),
    endpoint: typeof port === 'number' && port > 0 ? `127.0.0.1:${port}` : null,
    state: input.status?.state,
    token,
    maskedToken: token ? maskLocalToken(token) : null,
    unavailable: input.unavailable,
    targetAgentId: input.targetAgentId,
  };
}

/**
 * One row per default-pool endpoint (Responses split into Codex / Grok),
 * plus leftover local-bridge listeners not covered by a pool.
 */
export function buildLocalTokenRows(
  profiles: readonly AdapterProfile[],
  bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus | undefined>,
  statusErrors: Readonly<Record<string, unknown>> = {},
  pools: readonly DefaultRoutePoolOverview[] = [],
  chatCompletionsShared = false,
): LocalTokenRow[] {
  const covered = new Set<string>();
  const rows: LocalTokenRow[] = [];
  const sharedKimiChat = chatCompletionsShared && pools.some((pool) => (
    pool.targetAgentId === 'kimi'
    && pool.surface === 'chat_completions'
    && pool.members.length > 0
  ));
  let sharedChatRow = false;

  for (const pool of pools) {
    if (pool.members.length === 0) continue;
    const kind = localEndpointKindFromPool(pool);
    if (!kind) continue;
    if (
      chatCompletionsShared
      && kind === 'chat_completions'
      && (sharedChatRow || (sharedKimiChat && pool.targetAgentId === 'dsh'))
    ) {
      const matches = profilesForPool(pool, profiles);
      for (const match of matches) covered.add(match.id);
      continue;
    }
    const matches = profilesForPool(pool, profiles);
    for (const match of matches) covered.add(match.id);
    const profile = pickRuntimeProfile(matches, bridgeStatuses);
    const statusId = profile?.id ?? pool.id;
    rows.push(rowFromRuntime({
      id: pool.id,
      name: `${pool.targetAgentId} · ${localEndpointPath(kind)}`,
      kind,
      targetAgentId: pool.targetAgentId,
      profile,
      portHint: pool.gatewayPort,
      status: bridgeStatuses[statusId],
      unavailable: Boolean(statusErrors[statusId]),
    }));
    if (kind === 'chat_completions') sharedChatRow = true;
  }

  for (const profile of profiles) {
    if (profile.route !== 'local_bridge') continue;
    if (covered.has(profile.id)) continue;
    const kind = localEndpointKindForTargetAgent(profile.targetAgentId);
    if (chatCompletionsShared && kind === 'chat_completions' && sharedChatRow) continue;
    const status = bridgeStatuses[profile.id];
    const unavailable = Boolean(statusErrors[profile.id])
      || status?.upstreamStatus === 'unavailable';
    rows.push(rowFromRuntime({
      id: profile.id,
      name: profile.name.trim() || profile.targetAgentId,
      kind,
      targetAgentId: profile.targetAgentId,
      profile,
      portHint: profile.localPort,
      status,
      unavailable,
    }));
    if (kind === 'chat_completions') sharedChatRow = true;
  }

  return rows.sort((a, b) => {
    const kindOrder = a.kind.localeCompare(b.kind);
    if (kindOrder !== 0) return kindOrder;
    return a.name.localeCompare(b.name);
  });
}
