/**
 * Credential-family taxonomy shared by Connections / Accounts / Adapter.
 *
 * Canonical kind tokens: `oauth` | `apikey`.
 * Adapter profile wire mode still uses `api` (backend contract); map only at edges.
 */

export type ConnectionKind = 'oauth' | 'apikey';

export type ConnectionKindFilter = 'all' | ConnectionKind;

export const CONNECTION_KIND_FILTERS: Array<{ value: ConnectionKindFilter; label: string }> = [
  { value: 'all', label: '全部' },
  { value: 'oauth', label: '官方登录' },
  { value: 'apikey', label: 'API Key' },
];

export function connectionKindLabel(kind: ConnectionKind): string {
  return kind === 'oauth' ? '官方登录' : 'API Key';
}

export function connectionKindFilterLabel(filter: ConnectionKindFilter): string {
  if (filter === 'all') return '全部';
  return connectionKindLabel(filter);
}

export function kindBadge(kind: ConnectionKind): {
  label: string;
  variant: 'default' | 'info' | 'accent';
} {
  return kind === 'oauth'
    ? { label: connectionKindLabel('oauth'), variant: 'default' }
    : { label: connectionKindLabel('apikey'), variant: 'info' };
}

/** Search synonyms so "api" / "官方登录" both match. */
export function connectionKindSearchText(kind: ConnectionKind): string {
  return kind === 'oauth' ? '官方登录 oauth' : 'api key apikey';
}

/**
 * Parse page filter values. Accepts legacy aliases:
 * - apikey family: api | apikey | key | provider | providers
 * - oauth family: oauth | account | accounts
 */
export function parseConnectionKindFilter(raw: string | null | undefined): ConnectionKindFilter {
  if (!raw) return 'all';
  const value = raw.trim().toLowerCase();
  if (value === 'oauth' || value === 'account' || value === 'accounts') return 'oauth';
  if (
    value === 'apikey' ||
    value === 'api' ||
    value === 'key' ||
    value === 'provider' ||
    value === 'providers'
  ) {
    return 'apikey';
  }
  return 'all';
}

/** Deep-link focus: only return a concrete kind when the raw value is recognized. */
export function parseConnectionFocusFilter(raw: string | null | undefined): ConnectionKind | null {
  const filter = parseConnectionKindFilter(raw);
  return filter === 'all' ? null : filter;
}

export function filterByConnectionKind<T>(
  items: readonly T[],
  filter: ConnectionKindFilter,
  getKind: (item: T) => ConnectionKind,
): T[] {
  if (filter === 'all') return [...items];
  return items.filter((item) => getKind(item) === filter);
}

export function countByConnectionKind<T>(
  items: readonly T[],
  getKind: (item: T) => ConnectionKind,
): Record<ConnectionKindFilter, number> {
  const counts: Record<ConnectionKindFilter, number> = {
    all: items.length,
    oauth: 0,
    apikey: 0,
  };
  for (const item of items) {
    counts[getKind(item)] += 1;
  }
  return counts;
}

/** Backend AdapterProfileMode uses `api`; UI kind uses `apikey`. */
export function connectionKindFromAdapterProfileMode(mode: 'api' | 'oauth'): ConnectionKind {
  return mode === 'oauth' ? 'oauth' : 'apikey';
}

export function adapterProfileModeFromConnectionKind(kind: ConnectionKind): 'api' | 'oauth' {
  return kind === 'oauth' ? 'oauth' : 'api';
}
