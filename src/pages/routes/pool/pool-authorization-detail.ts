import { agentDisplayName } from '@/config/agents';
import type { TranslateFn } from '@/lib/i18n';
import {
  localEndpointBrandAgentId,
  localEndpointPath,
  type LocalEndpointKind,
} from '@/lib/route-endpoints';
import { agentCssVar } from '@/styles/tokens';
import {
  localEndpointKindLabel,
  type PoolAuthorizationItem,
} from '@/pages/bridges/route-pool-view-model';

export type PoolAuthorizationDetailRow = {
  id: string;
  label: string;
  value: string;
  /** Extra lines under `value` (e.g. more endpoint types). */
  lines?: string[];
  /** http(s) URL opened in the system browser. */
  href?: string;
  mono?: boolean;
  copyable?: boolean;
};

export function hasQuotaWindow(pct?: number): boolean {
  return typeof pct === 'number' && Number.isFinite(pct);
}

/** One model id per line or comma. Empty lines dropped. */
export function parseCustomModelList(raw: string): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const part of raw.split(/[\n,]/)) {
    const id = part.trim();
    if (!id || seen.has(id)) continue;
    seen.add(id);
    out.push(id);
  }
  return out;
}

export function formatPoolTimestamp(raw?: string | null): string | null {
  if (!raw?.trim()) return null;
  const value = raw.trim();
  const parsed = new Date(value.includes('T') ? value : value.replace(' ', 'T'));
  if (Number.isNaN(parsed.getTime())) return value;
  const y = parsed.getFullYear();
  const m = String(parsed.getMonth() + 1).padStart(2, '0');
  const d = String(parsed.getDate()).padStart(2, '0');
  const hh = String(parsed.getHours()).padStart(2, '0');
  const mm = String(parsed.getMinutes()).padStart(2, '0');
  return `${y}-${m}-${d} ${hh}:${mm}`;
}

export type PoolAuthorizationColumnKey =
  | 'enabled'
  | 'login'
  | 'kind'
  | 'endpointTypes'
  | 'status'
  | 'bindings'
  | 'quota'
  | 'lastUsed'
  | 'priority';

export const POOL_AUTHORIZATION_ALWAYS_COLUMNS: readonly PoolAuthorizationColumnKey[] = [
  'login',
  'kind',
  'endpointTypes',
  'status',
];

export function poolAuthorizationQuotaParts(
  item: Pick<PoolAuthorizationItem, 'quota5hPct' | 'quota7dPct'>,
): string[] {
  const parts: string[] = [];
  if (hasQuotaWindow(item.quota7dPct)) parts.push(`7d ${item.quota7dPct}%`);
  if (hasQuotaWindow(item.quota5hPct)) parts.push(`5h ${item.quota5hPct}%`);
  return parts;
}

export function poolAuthorizationVisibleColumns(
  items: readonly PoolAuthorizationItem[],
): PoolAuthorizationColumnKey[] {
  const columns: PoolAuthorizationColumnKey[] = [...POOL_AUTHORIZATION_ALWAYS_COLUMNS];
  if (items.some((item) => (item.bindingCount ?? 0) > 0)) columns.push('bindings');
  if (items.some((item) => poolAuthorizationQuotaParts(item).length > 0)) columns.push('quota');
  if (items.some((item) => Boolean(item.lastUsedAt?.trim()))) columns.push('lastUsed');
  if (items.some((item) => item.priority != null)) columns.push('priority');
  columns.push('enabled');
  return columns;
}

export function poolAuthorizationColumnLabel(
  key: PoolAuthorizationColumnKey,
  t: TranslateFn,
): string {
  switch (key) {
    case 'enabled':
      return t('routes.pool.detail.enabled');
    case 'login':
      return t('routes.pool.table.login');
    case 'kind':
      return t('routes.pool.table.kind');
    case 'endpointTypes':
      return t('routes.pool.detail.endpointTypes');
    case 'status':
      return t('routes.pool.table.status');
    case 'bindings':
      return t('routes.pool.detail.bindings');
    case 'quota':
      return t('routes.pool.detail.quota');
    case 'lastUsed':
      return t('connections.list.lastUsedAt');
    case 'priority':
      return t('routes.pool.detail.priority');
  }
}

export function poolAuthorizationEndpointKinds(
  item: Pick<PoolAuthorizationItem, 'endpointKinds' | 'surface' | 'agentId'>,
): LocalEndpointKind[] {
  if (item.endpointKinds.length > 0) return [...item.endpointKinds];
  if (!item.surface) return [];
  if (item.surface === 'messages') return ['messages'];
  if (item.surface === 'chat_completions') return ['chat_completions'];
  return [item.agentId === 'grok' ? 'responses_grok' : 'responses_codex'];
}

/** Human-readable endpoint types for one pool login (paths + dialect labels). */
export function poolAuthorizationEndpointTypeLabels(
  item: Pick<PoolAuthorizationItem, 'endpointKinds' | 'surface' | 'agentId'>,
  t: TranslateFn,
): string[] {
  return poolAuthorizationEndpointKinds(item).map((kind) => {
    const path = localEndpointPath(kind);
    const label = localEndpointKindLabel(kind, t);
    return `${path}（${label}）`;
  });
}

/** Endpoint-type brand colors for a URL login link icon (deduped, CSS vars). */
export function poolAuthorizationLinkIconColors(
  item: Pick<PoolAuthorizationItem, 'endpointKinds' | 'surface' | 'agentId'>,
): string[] {
  const seen = new Set<string>();
  const colors: string[] = [];
  for (const kind of poolAuthorizationEndpointKinds(item)) {
    const color = agentCssVar(localEndpointBrandAgentId(kind));
    if (seen.has(color)) continue;
    seen.add(color);
    colors.push(color);
  }
  return colors;
}

/** Hostname only; custom logins do not need the path. */
export function poolAuthorizationDomain(host?: string | null): string | null {
  const raw = host?.trim();
  if (!raw) return null;
  try {
    const url = /^https?:\/\//i.test(raw) ? new URL(raw) : new URL(`https://${raw}`);
    if (url.hostname) return url.hostname;
  } catch {
    /* fall through */
  }
  return raw.split('/')[0] || raw;
}

/** Full http(s) URL for the stored endpoint host. */
export function poolAuthorizationEndpointHref(host?: string | null): string | null {
  const raw = host?.trim();
  if (!raw) return null;
  try {
    const url = /^https?:\/\//i.test(raw) ? new URL(raw) : new URL(`https://${raw}`);
    if (url.protocol !== 'http:' && url.protocol !== 'https:') return null;
    if (!url.hostname) return null;
    return url.toString().replace(/\/$/, '');
  } catch {
    return null;
  }
}

/** Origin of the stored endpoint + a local endpoint path. */
export function poolAuthorizationTypeHref(
  host: string | null | undefined,
  path: string,
): string | null {
  const base = poolAuthorizationEndpointHref(host);
  if (!base) return null;
  const suffix = path.startsWith('/') ? path : `/${path}`;
  try {
    return `${new URL(base).origin}${suffix}`;
  } catch {
    return null;
  }
}

/** List / detail title: custom API Key rows show the domain only. */
export function poolAuthorizationLoginLabel(
  item: Pick<PoolAuthorizationItem, 'identityLabel' | 'title' | 'kind' | 'endpointMode' | 'endpointHost'>,
): string {
  if (item.kind === 'apikey' && item.endpointMode === 'custom') {
    return poolAuthorizationDomain(item.endpointHost) || item.identityLabel || item.title;
  }
  return item.identityLabel ?? item.title;
}

export function poolAuthorizationDetailRows(
  item: PoolAuthorizationItem,
  t: TranslateFn,
): PoolAuthorizationDetailRow[] {
  const rows: PoolAuthorizationDetailRow[] = [];
  if (item.subscription?.trim()) {
    rows.push({
      id: 'subscription',
      label: t('connections.list.subscription'),
      value: item.subscription.trim(),
    });
  }
  if (item.endpointHost?.trim()) {
    const host = item.endpointHost.trim();
    const value = item.endpointMode === 'custom'
      ? (poolAuthorizationDomain(host) ?? host)
      : host;
    const href = poolAuthorizationEndpointHref(host);
    rows.push({
      id: 'endpoint',
      label: t('connections.list.endpoint'),
      value,
      href: href ?? undefined,
      mono: true,
      copyable: true,
    });
  }
  const endpointTypes = poolAuthorizationEndpointTypeLabels(item, t);
  if (endpointTypes.length > 0) {
    rows.push({
      id: 'endpointTypes',
      label: t('routes.pool.detail.endpointTypes'),
      value: endpointTypes[0]!,
      lines: endpointTypes.slice(1),
    });
  }
  const secretTail = item.kind === 'oauth' ? item.refreshTokenTail : item.secretTail;
  if (secretTail?.trim()) {
    rows.push({
      id: item.kind === 'oauth' ? 'refreshTokenTail' : 'secret',
      label: item.kind === 'oauth'
        ? t('routes.pool.detail.refreshTokenTail')
        : t('routes.pool.detail.secretTail'),
      value: secretTail.trim(),
      mono: true,
    });
  }
  const lastUsed = formatPoolTimestamp(item.lastUsedAt);
  if (lastUsed) {
    rows.push({
      id: 'lastUsed',
      label: t('connections.list.lastUsedAt'),
      value: lastUsed,
    });
  }
  if (item.bindingCount && item.bindingCount > 0) {
    rows.push({
      id: 'bindings',
      label: t('routes.pool.detail.bindings'),
      value: t('routes.pool.detail.bindingCount', { count: item.bindingCount }),
    });
  }
  if (item.canToggle && item.priority != null) {
    rows.push({
      id: 'priority',
      label: t('routes.pool.detail.priority'),
      value: String(item.priority),
    });
  }
  rows.push({
    id: 'source',
    label: t('routes.pool.detail.source'),
    value: item.addedHere
      ? t('routes.pool.page.addedHere')
      : `${t('routes.pool.page.fromConnections')} · ${agentDisplayName(item.agentId)}`,
  });
  return rows;
}
