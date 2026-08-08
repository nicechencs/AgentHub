/**
 * Connections 统一列表模型：账号池 + 供应商池 → 同一行语义。
 * 产品层：供应商已并入「API Key」（官方端点 / 自定义端点）；存储仍分表。
 */
import {
  extractProviderEndpoint,
  formatApiConnectionLabel,
  formatEndpointHost,
} from '@/lib/api/agent-connection';
import { looksLikeOfficialEndpoint } from '@/config/official-api';
import type { Account, AgentId, AuthStatus, Provider } from '@/lib/types';
import { fmtRemaining } from '@/lib/utils';

/**
 * 列表行类型。
 * - oauth：官方登录
 * - apikey：API Key（含原 account apikey + 原 provider/供应商）
 */
export type ConnectionKind = 'oauth' | 'apikey';

export type ConnectionFilter = 'all' | ConnectionKind;

export const CONNECTION_FILTERS: Array<{ value: ConnectionFilter; label: string }> = [
  { value: 'all', label: '全部' },
  { value: 'oauth', label: '官方登录' },
  { value: 'apikey', label: 'API Key' },
];

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
  sortKey: string;
  identityLabel?: string;
  subscription?: string;
  quota5hPct?: number;
  quotaResetIn?: string;
  latencyMs?: number;
  endpointHost?: string;
  /**
   * API Key 子类型：官方端点 / 自定义中转。
   * oauth 行无此字段。
   */
  endpointMode?: 'official' | 'custom';
  account?: Account;
  provider?: Provider;
};

export function authStatusOfAccount(a: Account): AuthStatus {
  if (!a.tokenValid) return 'expired';
  if (a.tokenRemainingSec === undefined) return a.kind === 'apikey' ? 'valid' : 'none';
  if (a.tokenRemainingSec <= 3 * 3600) return 'expiring';
  return 'valid';
}

function accountSubtitle(a: Account): string {
  if (a.isCurrent) {
    if (!a.tokenValid) return 'token 已失效';
    if (a.tokenRemainingSec !== undefined) {
      return `token 剩余 ${fmtRemaining(a.tokenRemainingSec)}`;
    }
    return a.kind === 'apikey' ? 'API Key · 当前生效' : 'token 有效';
  }
  const bits: string[] = [];
  if (a.kind === 'apikey') bits.push('API Key');
  if (a.identityLabel && a.identityLabel !== a.label) bits.push(a.identityLabel);
  if (a.source) bits.push(a.source);
  return bits.length ? bits.join(' · ') : '未生效';
}

function providerEndpointMode(p: Provider, endpoint?: string): 'official' | 'custom' {
  if (p.official === true) return 'official';
  if (p.official === false) return 'custom';
  if (p.preset && /anthropic|openai|moonshot|xai/i.test(p.preset) && !/compat|custom|relay/i.test(p.preset)) {
    // 官方预设且无自定义 host 倾向 official
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
  if (p.isCurrent) {
    return host
      ? `当前生效 · ${modeLabel} · ${host}`
      : `当前生效 · ${modeLabel}`;
  }
  return host ? `未生效 · ${modeLabel} · ${host}` : `未生效 · ${modeLabel}`;
}

export function accountToEntry(a: Account): ConnectionEntry {
  return {
    key: `account:${a.id}`,
    source: 'account',
    kind: a.kind === 'apikey' ? 'apikey' : 'oauth',
    id: a.id,
    agentId: a.agentId,
    title: a.label,
    subtitle: accountSubtitle(a),
    isCurrent: a.isCurrent,
    authStatus: authStatusOfAccount(a),
    sortKey: a.updatedAt || a.lastUsedAt || a.createdAt || '',
    identityLabel: a.identityLabel,
    subscription: a.subscription,
    quota5hPct: a.quota5hPct,
    quotaResetIn: a.quotaResetIn,
    // 账号池 API Key 默认视为官方直连（无 endpoint 字段）
    endpointMode: a.kind === 'apikey' ? 'official' : undefined,
    account: a,
  };
}

export function providerToEntry(p: Provider): ConnectionEntry {
  const endpoint = extractProviderEndpoint(p.configText, p.configFormat);
  const endpointMode = providerEndpointMode(p, endpoint);
  return {
    key: `provider:${p.id}`,
    source: 'provider',
    // 产品：供应商并入 API Key
    kind: 'apikey',
    id: p.id,
    agentId: p.agentId,
    title: p.name,
    subtitle: providerSubtitle(p, endpoint, endpointMode),
    isCurrent: p.isCurrent,
    authStatus: 'valid',
    sortKey: p.updatedAt || '',
    latencyMs: p.latencyMs,
    endpointHost: endpoint ? formatEndpointHost(endpoint) : undefined,
    endpointMode,
    provider: p,
  };
}

/** 合并两池：当前项优先，再按更新时间降序 */
export function mergeConnectionEntries(
  accounts: Account[],
  providers: Provider[],
): ConnectionEntry[] {
  const rows: ConnectionEntry[] = [
    ...accounts.map(accountToEntry),
    ...providers.map(providerToEntry),
  ];
  rows.sort((a, b) => {
    if (a.isCurrent !== b.isCurrent) return a.isCurrent ? -1 : 1;
    if (a.sortKey !== b.sortKey) return a.sortKey < b.sortKey ? 1 : -1;
    return a.key.localeCompare(b.key);
  });
  return rows;
}

export function filterConnectionEntries(
  rows: ConnectionEntry[],
  filter: ConnectionFilter,
): ConnectionEntry[] {
  if (filter === 'all') return rows;
  return rows.filter((r) => r.kind === filter);
}

export function countByKind(rows: ConnectionEntry[]): Record<ConnectionFilter, number> {
  const counts: Record<ConnectionFilter, number> = {
    all: rows.length,
    oauth: 0,
    apikey: 0,
  };
  for (const r of rows) {
    counts[r.kind] += 1;
  }
  return counts;
}

export function kindBadge(kind: ConnectionKind): {
  label: string;
  variant: 'default' | 'info' | 'accent';
} {
  switch (kind) {
    case 'oauth':
      return { label: '官方登录', variant: 'default' };
    case 'apikey':
      return { label: 'API Key', variant: 'info' };
  }
}

export function endpointModeBadge(
  mode: 'official' | 'custom' | undefined,
): { label: string; variant: 'default' | 'info' } | null {
  if (mode === 'official') return { label: '官方', variant: 'default' };
  if (mode === 'custom') return { label: '自定义', variant: 'info' };
  return null;
}

export function withProviderLatency(
  entry: ConnectionEntry,
  latencyMs: number | undefined,
): ConnectionEntry {
  if (entry.source !== 'provider' || latencyMs === undefined) return entry;
  const modeLabel = entry.endpointMode === 'custom' ? '自定义端点' : '官方端点';
  const host = entry.endpointHost;
  const base = entry.isCurrent
    ? host
      ? `当前生效 · ${modeLabel} · ${host}`
      : `当前生效 · ${modeLabel}`
    : host
      ? `未生效 · ${modeLabel} · ${host}`
      : `未生效 · ${modeLabel}`;
  return {
    ...entry,
    latencyMs,
    subtitle: `${base} · ${latencyMs} ms`,
  };
}

export function providerDisplayLabel(p: Provider): string {
  return formatApiConnectionLabel(p);
}
