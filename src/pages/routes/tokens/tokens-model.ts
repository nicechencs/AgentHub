/**
 * Pure view-model for the local-tokens page. No React, no IO.
 */
import type {
  AdapterBridgeRuntimeState,
  AdapterBridgeRuntimeStatus,
  AdapterProfile,
  DefaultRoutePoolOverview,
  LocalTokenRecord,
} from '@/lib/backend/contracts/adapter';
import type { TranslateFn } from '@/lib/i18n';
import { KNOWN_AGENT_IDS, type AgentKey } from '@/lib/types';
import {
  LOCAL_ENDPOINT_KINDS,
  localEndpointKindForTargetAgent,
  localEndpointKindFromPool,
  localEndpointPath,
  type LocalEndpointKind,
} from '@/lib/route-endpoints';
import { agentConversationSurfaces } from '@/pages/agents/agent-detail-model';
import { localEndpointKindLabel } from '@/pages/routes/shared/route-pool-view-model';
import { profilesForPool } from '@/pages/routes/board/board-view-model';

export interface LocalTokenRow {
  /** Stable list key: pool id (pool-backed) or profile id (leftover). */
  id: string;
  /**
   * True when `id` is a route pool id safe for `setLocalToken`.
   * False for leftover local_bridge profiles not covered by a pool —
   * those keep `id === profile.id` for list stability but must not call setLocalToken.
   */
  poolBacked: boolean;
  profileId: string | null;
  /** Profiles whose gateway usage belongs to this entry key. */
  profileIds: string[];
  name: string;
  /** True for the type's default pool key. */
  primary: boolean;
  /** Pool-backed keys can be deleted; leftover listener rows cannot. */
  canDelete: boolean;
  kind: LocalEndpointKind;
  path: string;
  endpoint: string | null;
  state: AdapterBridgeRuntimeState | undefined;
  /** Loopback bearer (`ahb_…`); null until the pool has an entry key. */
  token: string | null;
  /** Safe default display; the raw token remains available only for copy/reveal. */
  maskedToken: string | null;
  /** The runtime status read failed, so no token action is safe. */
  unavailable: boolean;
  /** Pool/runtime writer this key belongs to; not a client-write target. */
  targetAgentId: string;
  /** Models this entry key can send; first item is the default test pick. */
  listedModels: string[];
}

export function uniqueListedModels(
  values: readonly (string | null | undefined)[],
): string[] {
  const seen = new Set<string>();
  const listed: string[] = [];
  for (const value of values) {
    const model = value?.trim();
    if (!model || seen.has(model)) continue;
    seen.add(model);
    listed.push(model);
  }
  return listed;
}

export function maskLocalToken(token: string): string {
  const trimmed = token.trim();
  if (!trimmed) return '';
  const tail = trimmed.slice(-4);
  return trimmed.startsWith('ahb_') ? `ahb_••••${tail}` : `••••${tail}`;
}

/** Same shape as core `generate_hub_token`: `ahb_` + 32 random bytes, base64url, no pad. */
export function generateLocalToken(
  randomBytes: (size: number) => Uint8Array = defaultRandomBytes,
): string {
  const bytes = randomBytes(32);
  let binary = '';
  for (const byte of bytes) binary += String.fromCharCode(byte);
  const encoded = btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
  return `ahb_${encoded}`;
}

function defaultRandomBytes(size: number): Uint8Array {
  const bytes = new Uint8Array(size);
  crypto.getRandomValues(bytes);
  return bytes;
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

/** Agents that speak this endpoint. Grok stays on Grok Responses only. */
export function agentSupportsLocalEndpointKind(
  agentId: string,
  kind: LocalEndpointKind,
): boolean {
  if (kind === 'responses_grok') return agentId === 'grok';
  if (agentId === 'grok') return false;
  const surface = kind === 'messages'
    ? 'messages'
    : kind === 'chat_completions'
      ? 'chat_completions'
      : 'responses';
  return agentConversationSurfaces(agentId).includes(surface);
}

export function supportedAgentsForEndpointKind(kind: LocalEndpointKind): AgentKey[] {
  return KNOWN_AGENT_IDS.filter((id) => agentSupportsLocalEndpointKind(id, kind));
}

export type CreateTokenEndpointCard = {
  kind: LocalEndpointKind;
  path: string;
  /** Primary pool id when this endpoint can receive a new key. */
  poolId: string | null;
  agentIds: readonly AgentKey[];
};

/** Four endpoint cards for 新建入口 Key; missing pools stay visible and unselectable. */
export function buildCreateTokenEndpointCards(
  targets: readonly Pick<LocalTokenRow, 'id' | 'kind'>[],
): CreateTokenEndpointCard[] {
  const poolByKind = new Map<LocalEndpointKind, string>();
  for (const row of targets) {
    if (!poolByKind.has(row.kind)) poolByKind.set(row.kind, row.id);
  }
  return LOCAL_ENDPOINT_KINDS.map((endpoint) => ({
    kind: endpoint.kind,
    path: endpoint.path,
    poolId: poolByKind.get(endpoint.kind) ?? null,
    agentIds: supportedAgentsForEndpointKind(endpoint.kind),
  }));
}

export function firstCreateTokenPoolId(
  cards: readonly CreateTokenEndpointCard[],
): string {
  return cards.find((card) => card.poolId)?.poolId ?? '';
}

/** Name used when 新建 leaves the field empty. Numbered from 2 because the type already has a default key. */
export function defaultCreateTokenName(input: {
  kind: LocalEndpointKind;
  existingNames?: readonly string[];
  t?: TranslateFn;
}): string {
  const label = localEndpointKindLabel(input.kind, input.t).trim();
  const used = new Set(
    (input.existingNames ?? [])
      .map((name) => name.trim())
      .filter(Boolean),
  );
  let n = 2;
  let candidate = `${label} ${n}`;
  while (used.has(candidate)) {
    n += 1;
    candidate = `${label} ${n}`;
  }
  return candidate;
}

/** Outbound entry-key kinds visible on the board; hidden Agents are omitted. */
export function visibleTokenKinds(
  rows: readonly Pick<LocalTokenRow, 'kind' | 'targetAgentId'>[],
  hiddenAgentIds: ReadonlySet<string> = new Set(),
): LocalEndpointKind[] {
  return rows
    .filter((row) => !hiddenAgentIds.has(row.targetAgentId))
    .map((row) => row.kind);
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

function profileIdsForPool(
  pool: Pick<DefaultRoutePoolOverview, 'id' | 'targetAgentId' | 'members'>,
  profiles: readonly AdapterProfile[],
): string[] {
  const matches = profilesForPool(pool, profiles);
  return [...new Set([pool.id, ...matches.map((item) => item.id)])];
}

export function tokenDisplayName(
  row: Pick<LocalTokenRow, 'name' | 'kind' | 'primary'>,
  t?: TranslateFn,
): string {
  const name = row.name.trim();
  if (name) return name;
  if (row.primary) return t ? t('routes.tokens.defaultName') : '默认';
  return tokenTypeLabel(row, t);
}

export type LocalTokenGroup = {
  kind: LocalEndpointKind;
  path: string;
  /** Shared loopback address for this type; taken from the first row. */
  endpoint: string | null;
  rows: LocalTokenRow[];
};

/** Group list rows by endpoint type, keeping the existing kind / default / name order. */
export function buildLocalTokenGroups(
  rows: readonly LocalTokenRow[],
): LocalTokenGroup[] {
  const grouped = new Map<LocalEndpointKind, LocalTokenRow[]>();
  for (const row of rows) {
    const list = grouped.get(row.kind);
    if (list) list.push(row);
    else grouped.set(row.kind, [row]);
  }
  return LOCAL_ENDPOINT_KINDS.flatMap((spec) => {
    const groupRows = grouped.get(spec.kind);
    if (!groupRows || groupRows.length === 0) return [];
    return [{
      kind: spec.kind,
      path: spec.path,
      endpoint: groupRows[0]?.endpoint ?? null,
      rows: groupRows,
    }];
  });
}

function rowFromRuntime(input: {
  id: string;
  poolBacked: boolean;
  primary: boolean;
  canDelete: boolean;
  name: string;
  kind: LocalEndpointKind;
  targetAgentId: string;
  profile: AdapterProfile | null;
  profileIds: readonly string[];
  portHint: number | null | undefined;
  status: AdapterBridgeRuntimeStatus | undefined;
  unavailable: boolean;
  storedToken?: string | null;
  listedModels?: readonly string[];
}): LocalTokenRow {
  const port = input.unavailable
    ? null
    : (input.status?.port ?? input.portHint ?? input.profile?.localPort);
  // Live listener bearer authenticates; pool hub_token often differs and 401s.
  // Copy / import / test must use runtime localToken while the entry is up.
  // Never fall back to a divergent stored hub_token on a live listener.
  const useRuntimeToken = input.primary || !input.poolBacked;
  const entryUp = input.status?.state === 'running' || input.status?.state === 'degraded';
  const runtimeToken = input.unavailable || !useRuntimeToken
    ? null
    : (input.status?.localToken?.trim() || null);
  const storedToken = input.storedToken?.trim() || null;
  const token = useRuntimeToken && entryUp && !input.unavailable
    ? runtimeToken
    : (runtimeToken || storedToken);
  return {
    id: input.id,
    poolBacked: input.poolBacked,
    primary: input.primary,
    canDelete: input.canDelete,
    profileId: input.profile?.id ?? null,
    profileIds: [...input.profileIds],
    name: input.name,
    kind: input.kind,
    path: localEndpointPath(input.kind),
    endpoint: typeof port === 'number' && port > 0 ? `127.0.0.1:${port}` : null,
    state: input.status?.state,
    token,
    maskedToken: token ? maskLocalToken(token) : null,
    unavailable: input.unavailable,
    targetAgentId: input.targetAgentId,
    listedModels: uniqueListedModels(input.listedModels ?? []),
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
  tokensByPoolId: Readonly<Record<string, string>> = {},
  records?: readonly LocalTokenRecord[] | null,
): LocalTokenRow[] {
  const covered = new Set<string>();
  const rows: LocalTokenRow[] = [];
  const sharedKimiChat = chatCompletionsShared && pools.some((pool) => (
    pool.targetAgentId === 'kimi'
    && pool.surface === 'chat_completions'
    && pool.members.length > 0
  ));
  let sharedChatRow = false;
  const extraChatIds: string[] = [];
  const extraChatModels: string[] = [];

  for (const pool of pools) {
    if (pool.members.length === 0) continue;
    const kind = localEndpointKindFromPool(pool);
    if (!kind) continue;
    const profileIds = profileIdsForPool(pool, profiles);
    if (
      chatCompletionsShared
      && kind === 'chat_completions'
      && (sharedChatRow || (sharedKimiChat && pool.targetAgentId === 'dsh'))
    ) {
      const matches = profilesForPool(pool, profiles);
      for (const match of matches) covered.add(match.id);
      extraChatIds.push(...profileIds);
      extraChatModels.push(...(pool.listedModels ?? []));
      continue;
    }
    const matches = profilesForPool(pool, profiles);
    for (const match of matches) covered.add(match.id);
    const profile = pickRuntimeProfile(matches, bridgeStatuses);
    const statusId = profile?.id ?? pool.id;
    const recordsReady = records != null;
    const primaryRecord = recordsReady
      ? records.find((record) => record.primary && record.poolId === pool.id)
      : undefined;
    const extraRecords = recordsReady
      ? records.filter((record) => !record.primary && record.poolId === pool.id)
      : [];
    const storedToken = primaryRecord?.token
      ?? tokensByPoolId[pool.id]
      ?? tokensByPoolId[statusId];
    if (!recordsReady || primaryRecord) {
      rows.push(rowFromRuntime({
        id: pool.id,
        poolBacked: true,
        primary: true,
        canDelete: true,
        name: primaryRecord?.name ?? '',
        kind,
        targetAgentId: pool.targetAgentId,
        profile,
        profileIds,
        portHint: pool.gatewayPort,
        status: bridgeStatuses[statusId],
        unavailable: Boolean(statusErrors[statusId]),
        storedToken,
        listedModels: pool.listedModels,
      }));
    } else if (extraRecords.length === 0) {
      rows.push(rowFromRuntime({
        id: pool.id,
        poolBacked: true,
        primary: true,
        canDelete: false,
        name: '',
        kind,
        targetAgentId: pool.targetAgentId,
        profile,
        profileIds,
        portHint: pool.gatewayPort,
        status: bridgeStatuses[statusId],
        unavailable: Boolean(statusErrors[statusId]),
        listedModels: pool.listedModels,
      }));
    }
    for (const extra of extraRecords) {
      rows.push(rowFromRuntime({
        id: extra.id,
        poolBacked: true,
        primary: false,
        canDelete: true,
        name: extra.name,
        kind,
        targetAgentId: pool.targetAgentId,
        profile,
        profileIds,
        portHint: pool.gatewayPort,
        status: bridgeStatuses[statusId],
        unavailable: Boolean(statusErrors[statusId]),
        storedToken: extra.token,
        listedModels: pool.listedModels,
      }));
    }
    if (kind === 'chat_completions') sharedChatRow = true;
  }

  for (const profile of profiles) {
    if (profile.route !== 'local_bridge') continue;
    if (covered.has(profile.id)) continue;
    const kind = localEndpointKindForTargetAgent(profile.targetAgentId);
    if (chatCompletionsShared && kind === 'chat_completions' && sharedChatRow) {
      extraChatIds.push(profile.id);
      continue;
    }
    const status = bridgeStatuses[profile.id];
    const unavailable = Boolean(statusErrors[profile.id])
      || status?.upstreamStatus === 'unavailable';
    rows.push(rowFromRuntime({
      id: profile.id,
      poolBacked: false,
      primary: false,
      canDelete: false,
      name: profile.name.trim() || profile.targetAgentId,
      kind,
      targetAgentId: profile.targetAgentId,
      profile,
      profileIds: [profile.id],
      portHint: profile.localPort,
      status,
      unavailable,
      storedToken: tokensByPoolId[profile.id],
    }));
    if (kind === 'chat_completions') sharedChatRow = true;
  }

  if (chatCompletionsShared && extraChatIds.length > 0) {
    for (const row of rows) {
      if (row.kind !== 'chat_completions') continue;
      row.profileIds = [...new Set([...row.profileIds, ...extraChatIds])];
      row.listedModels = uniqueListedModels([
        ...row.listedModels,
        ...extraChatModels,
      ]);
    }
  }

  return rows.sort((a, b) => {
    const kindOrder = a.kind.localeCompare(b.kind);
    if (kindOrder !== 0) return kindOrder;
    if (a.primary !== b.primary) return a.primary ? -1 : 1;
    return tokenDisplayName(a).localeCompare(tokenDisplayName(b));
  });
}


/**
 * Whether「改入口 Key」may call `setLocalToken(row.id, …)`.
 * Only pool-backed rows have a real pool id; leftover profile rows must stay disabled.
 */
export type LocalTokenEditKeyGate = {
  enabled: boolean;
  reason: string | null;
};

export function localTokenDeleteGate(
  row: Pick<LocalTokenRow, 'canDelete' | 'poolBacked' | 'unavailable' | 'kind'>,
  rows: readonly Pick<LocalTokenRow, 'kind' | 'poolBacked'>[] = [],
  t?: TranslateFn,
): LocalTokenEditKeyGate {
  if (row.unavailable) {
    return {
      enabled: false,
      reason: t ? t('routes.runtime.unavailable') : '状态不可用',
    };
  }
  if (!row.poolBacked || !row.canDelete) {
    return {
      enabled: false,
      reason: t
        ? t('routes.tokens.editKeyNeedPool')
        : '这条还不是连接池入口 Key，先从路由建入口',
    };
  }
  const sameKind = rows.filter((item) => item.kind === row.kind && item.poolBacked);
  if (sameKind.length <= 1) {
    return {
      enabled: false,
      reason: t
        ? t('routes.tokens.deleteNeedExtra')
        : '这类型只剩这一把，不能删除，可修改',
    };
  }
  return { enabled: true, reason: null };
}

export function localTokenEmptyCreateGate(
  row: Pick<LocalTokenRow, 'poolBacked' | 'kind'>,
  createPoolIdByKind: Readonly<Partial<Record<LocalEndpointKind, string>>> = {},
  t?: TranslateFn,
  needRoute = false,
): LocalTokenEditKeyGate {
  if (row.poolBacked || createPoolIdByKind[row.kind]) {
    return { enabled: true, reason: null };
  }
  return {
    enabled: false,
    reason: t
      ? t(needRoute ? 'routes.board.entryNeedRoute' : 'routes.tokens.createNeedPool')
      : (needRoute
        ? '连接池已有登录。打开本机转发后，端点 Key 会出现在入口 Key 页'
        : '先在连接池加入登录'),
  };
}

export function localTokenEditKeyGate(
  row: Pick<LocalTokenRow, 'poolBacked' | 'unavailable'>,
  t?: TranslateFn,
): LocalTokenEditKeyGate {
  if (row.unavailable) {
    return {
      enabled: false,
      reason: t ? t('routes.runtime.unavailable') : '状态不可用',
    };
  }
  if (!row.poolBacked) {
    return {
      enabled: false,
      reason: t
        ? t('routes.tokens.editKeyNeedPool')
        : '这条还不是连接池入口 Key，先从路由建入口',
    };
  }
  return { enabled: true, reason: null };
}
