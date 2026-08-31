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

export function poolAuthorizationListChips(
  item: PoolAuthorizationItem,
  t: TranslateFn,
): string[] {
  const chips: string[] = [];
  const lastUsed = formatPoolTimestamp(item.lastUsedAt);
  if (lastUsed) chips.push(lastUsed);
  if (item.bindingCount && item.bindingCount > 0) {
    chips.push(t('routes.pool.detail.bindingCount', { count: item.bindingCount }));
  }
  if (hasQuotaWindow(item.quota7dPct)) chips.push(`7d ${item.quota7dPct}%`);
  if (hasQuotaWindow(item.quota5hPct)) chips.push(`5h ${item.quota5hPct}%`);
  if (item.canToggle && item.priority != null) {
    chips.push(t('routes.pool.detail.priorityValue', { n: item.priority }));
  }
  return chips;
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
  if (item.secretTail?.trim()) {
    rows.push({
      id: 'secret',
      label: t('routes.pool.detail.secretTail'),
      value: item.secretTail.trim(),
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
