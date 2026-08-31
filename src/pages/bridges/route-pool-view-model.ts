/**
 * Pure view-model for the Routes auth-pool workbench. No React, no IO.
 */
import type {
  AdapterBridgeRuntimeStatus,
  AdapterProfile,
  AdapterSourceKind,
  DefaultRoutePoolOverview,
  RouteMemberOverview,
  RoutePoolDialect,
  RoutePoolSurface,
} from '@/lib/backend/contracts/adapter';
import { authHealthLabel, type AuthHealth } from '@/lib/backend/contracts/auth-state';
import type { ConnectionEntry } from '@/lib/connection-entry';
import type { ConnectionKind } from '@/lib/connection-kind';
import type { AgentId, AuthStatus } from '@/lib/types';
import type { TranslateFn } from '@/lib/i18n';
import { groupLocalBridgeProfiles } from './adapter-view-model';

/** One workbench card: a default pool, or a local route not yet in a pool. */
export type PoolWorkbenchRow = {
  key: string;
  pool: DefaultRoutePoolOverview | null;
  profile: AdapterProfile | null;
  targetAgentId: AdapterProfile['targetAgentId'];
  surface: RoutePoolSurface | null;
  gatewayPort: number | null;
};

export function defaultPoolEntryUrl(port?: number | null): { url: string | null; pending: boolean } {
  if (typeof port === 'number' && port > 0) {
    return { url: `http://127.0.0.1:${port}`, pending: false };
  }
  return { url: null, pending: true };
}

export function routePoolMembersSectionVisible(
  flagOn: boolean,
  pool: DefaultRoutePoolOverview | null | undefined,
): boolean {
  return flagOn === true && pool != null;
}

export function nativeEnrollCtaVisible(input: {
  flagOn: boolean;
  route: AdapterProfile['route'];
  canApplyLocalBridge: boolean;
}): boolean {
  if (!input.flagOn) return false;
  if (input.route !== 'native_endpoint' && input.route !== 'config_sync') return false;
  return input.canApplyLocalBridge === true;
}

export function matchDefaultPoolForProfile(
  pools: readonly DefaultRoutePoolOverview[],
  profile: Pick<AdapterProfile, 'id' | 'sourceKind' | 'sourceId' | 'targetAgentId'>,
): DefaultRoutePoolOverview | null {
  const byId = pools.find((pool) => pool.id === profile.id);
  if (byId) return byId;
  return pools.find((pool) => (
    pool.targetAgentId === profile.targetAgentId
    && pool.members.some((member) => (
      member.sourceKind === profile.sourceKind && member.sourceId === profile.sourceId
    ))
  )) ?? null;
}

export function directProfilesForRoutePoolV2<T extends AdapterProfile>(
  flagOn: boolean,
  profiles: readonly T[],
  statuses: Record<string, AdapterBridgeRuntimeStatus> = {},
): T[] {
  if (!flagOn) return [];
  const filtered = profiles.filter((profile) => {
    if (profile.route !== 'native_endpoint' && profile.route !== 'config_sync') return false;
    return !profiles.some((sibling) => (
      sibling.route === 'local_bridge'
      && sibling.sourceKind === profile.sourceKind
      && sibling.sourceId === profile.sourceId
      && sibling.targetAgentId === profile.targetAgentId
    ));
  });
  return groupRouteProfilesBySource(filtered, statuses);
}

/** One list card per upstream source (native/config_sync/direct pool rows). */
export function groupRouteProfilesBySource<T extends AdapterProfile>(
  profiles: readonly T[],
  statuses: Record<string, AdapterBridgeRuntimeStatus> = {},
): T[] {
  return groupLocalBridgeProfiles(profiles, statuses);
}

export function routePoolMemberLabels(
  members: readonly RouteMemberOverview[],
  entries: readonly Pick<ConnectionEntry, 'source' | 'id' | 'title' | 'kind'>[],
): {
  title: string;
  enabled: boolean;
  availability?: RouteMemberOverview['availability'];
  sourceKind: RouteMemberOverview['sourceKind'];
  sourceId: string;
  kind?: ConnectionKind;
}[] {
  return members.map((member) => {
    const match = entries.find(
      (entry) => entry.source === member.sourceKind && entry.id === member.sourceId,
    );
    return {
      title: match?.title?.trim() || member.sourceId,
      enabled: member.enabled,
      availability: member.availability,
      sourceKind: member.sourceKind,
      sourceId: member.sourceId,
      kind: match?.kind,
    };
  });
}

export function poolSurfaceForAgent(agentId: AgentId): RoutePoolSurface | null {
  if (agentId === 'claude') return 'messages';
  if (agentId === 'codex' || agentId === 'grok') return 'responses';
  if (agentId === 'kimi' || agentId === 'dsh') return 'chat_completions';
  return null;
}

function poolDialectForAgent(agentId: AgentId): RoutePoolDialect {
  if (
    agentId === 'claude'
    || agentId === 'codex'
    || agentId === 'grok'
    || agentId === 'kimi'
    || agentId === 'dsh'
  ) {
    return agentId;
  }
  return 'generic';
}

function entryHome(entry: Pick<ConnectionEntry, 'account' | 'provider'>): 'route_pool' | undefined {
  return entry.account?.home ?? entry.provider?.home;
}

export type PoolAuthorizationItem = {
  key: string;
  sourceKind: AdapterSourceKind;
  sourceId: string;
  agentId: AgentId;
  title: string;
  kind: ConnectionKind;
  surface: RoutePoolSurface | null;
  addedHere: boolean;
  authHealth?: AuthHealth;
  authStatus?: AuthStatus;
};

function poolAuthorizationItem(
  key: string,
  sourceKind: AdapterSourceKind,
  sourceId: string,
  match: ConnectionEntry | undefined,
  fallback: {
    agentId: AgentId;
    kind: ConnectionKind;
    title: string;
    surface: RoutePoolSurface | null;
    addedHere: boolean;
  },
): PoolAuthorizationItem {
  return {
    key,
    sourceKind,
    sourceId,
    agentId: match?.agentId ?? fallback.agentId,
    title: match?.title?.trim() || fallback.title,
    kind: match?.kind ?? fallback.kind,
    surface: fallback.surface,
    addedHere: fallback.addedHere,
    authHealth: match?.authHealth,
    authStatus: match?.authStatus,
  };
}

/** Login-status chip for one authorization row. */
export function poolAuthorizationStatusView(
  item: Pick<PoolAuthorizationItem, 'authHealth' | 'authStatus'>,
  t?: TranslateFn,
): { label: string; tone: 'success' | 'warning' | 'danger' | 'info' | 'muted' } {
  const health: AuthHealth = item.authHealth
    ?? (item.authStatus === 'expired'
      ? 'needs_login'
      : item.authStatus === 'none'
        ? 'missing'
        : item.authStatus === 'expiring'
          ? 'unknown'
          : 'unknown');
  const tone = health === 'needs_login'
    ? 'danger'
    : health === 'missing' || health === 'unknown'
      ? 'muted'
      : 'success';
  return { label: authHealthLabel(health, t), tone };
}

/** Every OAuth / API authorization visible on the auth-pool page. */
export function collectPoolAuthorizations(
  pools: readonly DefaultRoutePoolOverview[],
  entries: readonly ConnectionEntry[],
): PoolAuthorizationItem[] {
  const items = new Map<string, PoolAuthorizationItem>();
  const entryBySource = new Map<string, ConnectionEntry>(
    entries.map((entry) => [`${entry.source}:${entry.id}`, entry]),
  );
  for (const pool of pools) {
    for (const member of pool.members) {
      const key = `${member.sourceKind}:${member.sourceId}`;
      const match = entryBySource.get(key);
      items.set(key, poolAuthorizationItem(
        key,
        member.sourceKind,
        member.sourceId,
        match,
        {
          agentId: pool.targetAgentId,
          kind: member.sourceKind === 'account' ? 'oauth' : 'apikey',
          title: member.sourceId,
          surface: pool.surface,
          addedHere: match ? entryHome(match) === 'route_pool' : false,
        },
      ));
    }
  }
  for (const entry of entries) {
    if (entryHome(entry) !== 'route_pool') continue;
    const key = `${entry.source}:${entry.id}`;
    if (items.has(key)) continue;
    items.set(key, poolAuthorizationItem(
      key,
      entry.source,
      entry.id,
      entry,
      {
        agentId: entry.agentId,
        kind: entry.kind,
        title: entry.id,
        surface: poolSurfaceForAgent(entry.agentId),
        addedHere: true,
      },
    ));
  }
  return [...items.values()].sort((left, right) => {
    const agent = left.agentId.localeCompare(right.agentId);
    if (agent !== 0) return agent;
    if (left.kind !== right.kind) return left.kind === 'oauth' ? -1 : 1;
    return left.title.localeCompare(right.title) || left.key.localeCompare(right.key);
  });
}

/** Fold pool-owned authorizations into workbench cards so they always appear. */
export function mergeOwnedAuthorizationsIntoRows(
  rows: PoolWorkbenchRow[],
  entries: readonly ConnectionEntry[],
): PoolWorkbenchRow[] {
  const next = rows.map((row) => (
    row.pool
      ? { ...row, pool: { ...row.pool, members: [...row.pool.members] } }
      : row
  ));
  for (const entry of entries) {
    if (entryHome(entry) !== 'route_pool') continue;
    const surface = poolSurfaceForAgent(entry.agentId);
    if (!surface) continue;
    const member: RouteMemberOverview = {
      sourceKind: entry.source,
      sourceId: entry.id,
      enabled: true,
    };
    const existing = next.find((row) => (
      row.targetAgentId === entry.agentId && row.surface === surface
    ));
    if (existing?.pool) {
      if (!existing.pool.members.some((item) => (
        item.sourceKind === member.sourceKind && item.sourceId === member.sourceId
      ))) {
        existing.pool.members.push(member);
      }
      continue;
    }
    if (existing) continue;
    next.push({
      key: `owned:${entry.agentId}:${surface}`,
      pool: {
        id: `owned:${entry.agentId}:${surface}`,
        targetAgentId: entry.agentId,
        surface,
        dialect: poolDialectForAgent(entry.agentId),
        v2Enrolled: false,
        members: [member],
        listedModels: [],
      },
      profile: null,
      targetAgentId: entry.agentId,
      surface,
      gatewayPort: null,
    });
  }
  return next;
}

export function routePoolSurfaceLabel(surface: RoutePoolSurface, t?: TranslateFn): string {
  if (surface === 'messages') {
    return t ? t('routes.pool.surface.messages') : '对话接口';
  }
  if (surface === 'responses') {
    return t ? t('routes.pool.surface.responses') : '回复接口';
  }
  return t ? t('routes.pool.surface.chatCompletions') : '对话补全';
}

function pickLeadProfile<T extends AdapterProfile>(
  members: readonly T[],
  statuses: Record<string, AdapterBridgeRuntimeStatus>,
): T {
  const running = members.find((member) => {
    const state = statuses[member.id]?.state;
    return state === 'running' || state === 'degraded';
  });
  if (running) return running;
  return [...members].sort((left, right) => {
    const created = left.createdAt.localeCompare(right.createdAt);
    return created !== 0 ? created : left.id.localeCompare(right.id);
  })[0]!;
}

export function leadProfileForPool(
  pool: Pick<DefaultRoutePoolOverview, 'id' | 'targetAgentId' | 'members'>,
  profiles: readonly AdapterProfile[],
  statuses: Record<string, AdapterBridgeRuntimeStatus> = {},
): AdapterProfile | null {
  const byId = profiles.find((profile) => profile.id === pool.id);
  if (byId) return byId;
  const matches = profiles.filter((profile) => (
    profile.route === 'local_bridge'
    && profile.targetAgentId === pool.targetAgentId
    && pool.members.some((member) => (
      member.sourceKind === profile.sourceKind && member.sourceId === profile.sourceId
    ))
  ));
  if (matches.length === 0) return null;
  return pickLeadProfile(matches, statuses);
}

export function localBridgesNotInPools(
  pools: readonly DefaultRoutePoolOverview[],
  profiles: readonly AdapterProfile[],
): AdapterProfile[] {
  return profiles.filter((profile) => (
    profile.route === 'local_bridge'
    && matchDefaultPoolForProfile(pools, profile) == null
  ));
}

function rowFromProfile(
  profile: AdapterProfile,
  statuses: Record<string, AdapterBridgeRuntimeStatus>,
): PoolWorkbenchRow {
  const statusPort = statuses[profile.id]?.port;
  const port = profile.localPort ?? (typeof statusPort === 'number' ? statusPort : null);
  return {
    key: profile.id,
    pool: null,
    profile,
    targetAgentId: profile.targetAgentId,
    surface: null,
    gatewayPort: typeof port === 'number' && port > 0 ? port : null,
  };
}

/** Cards for the auth-pool workbench. One row per Agent/surface pool, plus unmatched local routes. */
export function buildPoolWorkbenchRows(input: {
  flagOn: boolean;
  pools: readonly DefaultRoutePoolOverview[];
  profiles: readonly AdapterProfile[];
  statuses?: Record<string, AdapterBridgeRuntimeStatus>;
}): PoolWorkbenchRow[] {
  const statuses = input.statuses ?? {};
  if (!input.flagOn) {
    return input.profiles
      .filter((profile) => profile.route === 'local_bridge')
      .map((profile) => rowFromProfile(profile, statuses));
  }
  const fromPools = input.pools.map((pool) => {
    const profile = leadProfileForPool(pool, input.profiles, statuses);
    return {
      key: pool.id,
      pool,
      profile,
      targetAgentId: pool.targetAgentId,
      surface: pool.surface,
      gatewayPort: pool.gatewayPort ?? null,
    };
  });
  const extras = localBridgesNotInPools(input.pools, input.profiles)
    .map((profile) => rowFromProfile(profile, statuses));
  return [...fromPools, ...extras];
}
