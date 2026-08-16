/**
 * Shared connection list row. Used by Connections and Bridges.
 * Do not import this type from a page module.
 * Core fields come from `toCredentialRow` (P2-7); this type adds UI/list extras.
 */
import type { AuthHealth } from '@/lib/backend/contracts/auth-state';
import type { ConnectionKind } from '@/lib/connection-kind';
import {
  connectSourceKey,
  type ConnectionUsage,
  type ConnectionUsageMap,
} from '@/lib/connect-flow/types';
import {
  providerEndpointExtras,
  toCredentialRow,
} from '@/lib/credential-row';
import type { Account, AgentId, AuthStatus, Provider } from '@/lib/types';

export type ConnectionEntry = {
  /** 列表稳定 key：`account:id` / `provider:id` */
  key: string;
  source: 'account' | 'provider';
  kind: ConnectionKind;
  id: string;
  agentId: AgentId;
  title: string;
  subtitle: string;
  isCurrent: boolean;
  authStatus: AuthStatus;
  /** Optional for legacy hand-written entries; account/provider mappers fill it. */
  authHealth?: AuthHealth;
  sortKey: string;
  identityLabel?: string;
  subscription?: string;
  quota5hPct?: number;
  quota7dPct?: number;
  quotaResetIn?: string;
  quota7dResetIn?: string;
  latencyMs?: number;
  endpointHost?: string;
  /**
   * API Key 子类型：官方端点 / 自定义中转。
   * oauth 行无此字段。
   */
  endpointMode?: 'official' | 'custom';
  account?: Account;
  provider?: Provider;
  /** 钱包用途；由页面层计算后经 usageMap 填入，未接线时缺省 */
  usage?: ConnectionUsage;
};

export function authStatusOfAccount(a: Account): AuthStatus {
  return toCredentialRow({ source: 'account', account: a }).auth.status;
}

export function authHealthOfAccount(a: Account): AuthHealth {
  return toCredentialRow({ source: 'account', account: a }).auth.health ?? 'unknown';
}

function attachUsage(entry: ConnectionEntry, usageMap?: ConnectionUsageMap): ConnectionEntry {
  if (!usageMap) return entry;
  const usage = usageMap.get(connectSourceKey({ kind: entry.source, id: entry.id }));
  return usage === undefined ? entry : { ...entry, usage };
}

export function accountToEntry(a: Account, usageMap?: ConnectionUsageMap): ConnectionEntry {
  const row = toCredentialRow({ source: 'account', account: a });
  return attachUsage(
    {
      key: row.key,
      source: row.source,
      kind: a.kind === 'apikey' ? 'apikey' : 'oauth',
      id: row.id,
      agentId: row.agentId,
      title: row.title,
      subtitle: row.subtitle,
      isCurrent: row.isCurrent,
      authStatus: row.auth.status,
      authHealth: row.auth.health,
      sortKey: a.updatedAt || a.lastUsedAt || a.createdAt || '',
      identityLabel: a.identityLabel,
      subscription: a.subscription,
      quota5hPct: a.quota5hPct,
      quota7dPct: a.quota7dPct,
      quotaResetIn: a.quotaResetIn,
      quota7dResetIn: a.quota7dResetIn,
      endpointMode: a.kind === 'apikey' ? 'official' : undefined,
      account: a,
    },
    usageMap,
  );
}

export function providerToEntry(p: Provider, usageMap?: ConnectionUsageMap): ConnectionEntry {
  const row = toCredentialRow({ source: 'provider', provider: p });
  const { endpointHost, endpointMode } = providerEndpointExtras(p);
  return attachUsage(
    {
      key: row.key,
      source: row.source,
      kind: 'apikey',
      id: row.id,
      agentId: row.agentId,
      title: row.title,
      subtitle: row.subtitle,
      isCurrent: row.isCurrent,
      authStatus: row.auth.status,
      authHealth: row.auth.health,
      sortKey: p.updatedAt || '',
      latencyMs: p.latencyMs,
      endpointHost,
      endpointMode,
      provider: p,
    },
    usageMap,
  );
}

/** 合并两池：当前项优先，再按更新时间降序 */
export function mergeConnectionEntries(
  accounts: Account[],
  providers: Provider[],
  usageMap?: ConnectionUsageMap,
): ConnectionEntry[] {
  const rows: ConnectionEntry[] = [
    ...accounts.map((a) => accountToEntry(a, usageMap)),
    ...providers.map((p) => providerToEntry(p, usageMap)),
  ];
  rows.sort((a, b) => {
    if (a.isCurrent !== b.isCurrent) return a.isCurrent ? -1 : 1;
    if (a.sortKey !== b.sortKey) return a.sortKey < b.sortKey ? 1 : -1;
    return a.key.localeCompare(b.key);
  });
  return rows;
}
