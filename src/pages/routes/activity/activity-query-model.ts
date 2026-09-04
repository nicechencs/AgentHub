/**
 * Monitoring table query helpers: key / endpoint filters and pagination.
 */
import type { LocalTokenRecord } from '@/lib/backend/contracts/adapter';
import {
  isLocalEndpointKind,
  LOCAL_ENDPOINT_KINDS,
  type LocalEndpointKind,
} from '@/lib/route-endpoints';

export const ACTIVITY_PAGE_SIZE = 50;

export function parseActivityEndpointParam(value: string | null | undefined): LocalEndpointKind | null {
  const trimmed = value?.trim() ?? '';
  return isLocalEndpointKind(trimmed) ? trimmed : null;
}

export function parseActivityPageParam(value: string | null | undefined): number {
  const parsed = Number(value);
  if (!Number.isInteger(parsed) || parsed < 1) return 1;
  return parsed;
}

export function resolveActivityKeyQuery(
  tokenId: string | null | undefined,
  tokens: readonly LocalTokenRecord[],
): { keyLast4: string; poolId: string } | null {
  const id = tokenId?.trim() ?? '';
  if (!id) return null;
  const token = tokens.find((row) => row.id === id || row.poolId === id);
  const raw = token?.token.trim() ?? '';
  if (raw.length < 4) return null;
  return { keyLast4: raw.slice(-4), poolId: token?.poolId ?? id };
}

export function activityKeyOptionLabel(token: Pick<LocalTokenRecord, 'id' | 'token' | 'name'>): string {
  const name = token.name.trim();
  const raw = token.token.trim();
  const tail = raw.slice(-4);
  const abbrev = tail
    ? (raw.startsWith('ahb_') ? `ahb_••••${tail}` : `••••${tail}`)
    : '';
  return [abbrev, name].filter(Boolean).join(' ') || token.id;
}

export function activityEndpointKinds(): readonly LocalEndpointKind[] {
  return LOCAL_ENDPOINT_KINDS.map((item) => item.kind);
}

export function buildActivityPageItems(current: number, total: number): Array<number | 'ellipsis'> {
  if (total <= 0) return [];
  if (total <= 7) {
    return Array.from({ length: total }, (_, i) => i + 1);
  }
  const items: Array<number | 'ellipsis'> = [1];
  const left = Math.max(2, current - 1);
  const right = Math.min(total - 1, current + 1);
  if (left > 2) items.push('ellipsis');
  for (let page = left; page <= right; page += 1) items.push(page);
  if (right < total - 1) items.push('ellipsis');
  items.push(total);
  return items;
}

export function clampActivityPage(page: number, total: number, pageSize = ACTIVITY_PAGE_SIZE): number {
  const totalPages = Math.max(1, Math.ceil(Math.max(0, total) / pageSize));
  return Math.min(Math.max(1, page), totalPages);
}
