import type { TranslateFn } from '@/lib/i18n';
import type { PoolAuthorizationItem } from '@/pages/bridges/route-pool-view-model';

export type PoolAuthorizationDetailRow = {
  id: string;
  label: string;
  value: string;
  mono?: boolean;
  copyable?: boolean;
};

export function hasQuotaWindow(pct?: number): boolean {
  return typeof pct === 'number' && Number.isFinite(pct);
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
  | 'status'
  | 'bindings'
  | 'quota'
  | 'lastUsed'
  | 'priority';

export const POOL_AUTHORIZATION_ALWAYS_COLUMNS: readonly PoolAuthorizationColumnKey[] = [
  'login',
  'kind',
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
    rows.push({
      id: 'endpoint',
      label: t('connections.list.endpoint'),
      value: item.endpointHost.trim(),
      mono: true,
      copyable: true,
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
      : t('routes.pool.page.fromConnections'),
  });
  return rows;
}
