/**
 * Shared connection list row. Used by Connections and Bridges.
 * Do not import this type from a page module.
 */
import { looksLikeOfficialEndpoint } from '@/config/official-api';
import {
  extractProviderEndpoint,
  formatEndpointHost,
} from '@/lib/backend/contracts/agent-connection';
import {
  authDisplayForAccount,
  authHealthLabel,
  type AuthHealth,
} from '@/lib/backend/contracts/auth-state';
import type { ConnectionKind } from '@/lib/connection-kind';
import {
  connectSourceKey,
  type ConnectionUsage,
  type ConnectionUsageMap,
} from '@/lib/connect-flow/types';
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
  return authDisplayForAccount(a).legacyStatus;
}

export function authHealthOfAccount(a: Account): AuthHealth {
  return authDisplayForAccount(a).health;
}

function accountSubtitle(a: Account): string {
  if (a.isCurrent) {
    const bits: string[] = [];
    bits.push(authDisplayForAccount(a).label);
    if (a.subscription) bits.push(a.subscription);
    return bits.join(' · ');
  }
  const bits: string[] = [];
  bits.push(authDisplayForAccount(a).label, '未生效');
  if (a.provider && !a.label.includes(a.provider)) bits.push(a.provider);
  if (a.subscription) bits.push(a.subscription);
  return bits.join(' · ');
}

function providerEndpointMode(p: Provider, endpoint?: string): 'official' | 'custom' {
  if (p.official === true) return 'official';
  if (p.official === false) return 'custom';
  if (p.preset && /anthropic|openai|moonshot|xai/i.test(p.preset) && !/compat|custom|relay/i.test(p.preset)) {
    if (!endpoint || looksLikeOfficialEndpoint(p.agentId, endpoint)) return 'official';
  }
  if (!endpoint || looksLikeOfficialEndpoint(p.agentId, endpoint)) return 'official';
  return 'custom';
}

function providerSubtitle(
  p: Provider,
  endpoint: string | undefined,
  mode: 'official' | 'custom',
): string {
  const modeLabel = mode === 'official' ? '官方端点' : '自定义端点';
  const host = endpoint ? formatEndpointHost(endpoint) : undefined;
  const health = authHealthLabel('configured');
  if (p.isCurrent) {
    return host
      ? `${health} · 当前生效 · ${modeLabel} · ${host}`
      : `${health} · 当前生效 · ${modeLabel}`;
  }
  return host
    ? `${health} · 未生效 · ${modeLabel} · ${host}`
    : `${health} · 未生效 · ${modeLabel}`;
}

function attachUsage(entry: ConnectionEntry, usageMap?: ConnectionUsageMap): ConnectionEntry {
  if (!usageMap) return entry;
  const usage = usageMap.get(connectSourceKey({ kind: entry.source, id: entry.id }));
  return usage === undefined ? entry : { ...entry, usage };
}

export function accountToEntry(a: Account, usageMap?: ConnectionUsageMap): ConnectionEntry {
  return attachUsage(
    {
      key: `account:${a.id}`,
      source: 'account',
      kind: a.kind === 'apikey' ? 'apikey' : 'oauth',
      id: a.id,
      agentId: a.agentId,
      title: a.label,
      subtitle: accountSubtitle(a),
      isCurrent: a.isCurrent,
      authStatus: authStatusOfAccount(a),
      authHealth: authHealthOfAccount(a),
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
  const endpoint = extractProviderEndpoint(p.configText, p.configFormat);
  const endpointMode = providerEndpointMode(p, endpoint);
  return attachUsage(
    {
      key: `provider:${p.id}`,
      source: 'provider',
      kind: 'apikey',
      id: p.id,
      agentId: p.agentId,
      title: p.name,
      subtitle: providerSubtitle(p, endpoint, endpointMode),
      isCurrent: p.isCurrent,
      authStatus: 'valid',
      authHealth: 'configured',
      sortKey: p.updatedAt || '',
      latencyMs: p.latencyMs,
      endpointHost: endpoint ? formatEndpointHost(endpoint) : undefined,
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
