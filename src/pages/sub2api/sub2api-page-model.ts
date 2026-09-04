/** Pure helpers for the Sub2API routes page. */

import type { Sub2ApiKey, Sub2ApiSession, Sub2ApiUser } from '@/lib/sub2api';
import { SUB2API_DEFAULT_SITE_URL, normalizeSiteUrl } from '@/lib/sub2api';

export type Sub2ApiPagePhase = 'logged-out' | 'awaiting-2fa' | 'logged-in';

export type Sub2ApiKeyStatusKind = 'active' | 'disabled' | 'other';

export function sub2apiPagePhase(
  session: Sub2ApiSession | null,
  awaiting2fa: boolean,
): Sub2ApiPagePhase {
  if (session?.accessToken) return 'logged-in';
  if (awaiting2fa) return 'awaiting-2fa';
  return 'logged-out';
}

export function sub2apiDisplayName(
  user: Sub2ApiUser | null | undefined,
  session?: Sub2ApiSession | null,
): string {
  if (user) {
    const name = (user.display_name || user.username || user.email || '').trim();
    if (name) return name;
  }
  return (session?.user?.email || '').trim();
}

/** Normalize relay status strings/numbers into a coarse kind. */
export function sub2apiKeyStatusKind(status: unknown): Sub2ApiKeyStatusKind {
  const raw = String(status ?? '').trim().toLowerCase();
  if (raw === 'active' || raw === 'enabled' || raw === '1') return 'active';
  if (
    raw === 'disabled' ||
    raw === 'inactive' ||
    raw === '0' ||
    raw === '2' ||
    raw === 'banned' ||
    raw === 'expired'
  ) {
    return 'disabled';
  }
  return 'other';
}

export function sub2apiKeyStatusLabel(
  status: unknown,
  labels: { active: string; disabled: string; other: string },
): string {
  const kind = sub2apiKeyStatusKind(status);
  if (kind === 'active') return labels.active;
  if (kind === 'disabled') return labels.disabled;
  return labels.other;
}

export function initialSiteUrlDraft(session: Sub2ApiSession | null): string {
  return session?.siteUrl || SUB2API_DEFAULT_SITE_URL;
}

export function prepareSiteUrlForLogin(raw: string): string {
  return normalizeSiteUrl(raw || SUB2API_DEFAULT_SITE_URL);
}

export function sortSub2ApiKeys(keys: readonly Sub2ApiKey[]): Sub2ApiKey[] {
  return [...keys].sort((a, b) => {
    const an = (a.name || '').localeCompare(b.name || '', undefined, { sensitivity: 'base' });
    if (an !== 0) return an;
    return a.id - b.id;
  });
}

/** 6-digit TOTP only — strip non-digits and cap length. */
export function normalizeTotpCode(raw: string): string {
  return raw.replace(/\D/g, '').slice(0, 6);
}

/** Format ISO / date-like strings as `YYYY-MM-DD HH:mm` (local). Empty when missing/invalid. */
export function formatKeyTimestamp(raw: unknown): string | null {
  if (raw == null) return null;
  if (typeof raw !== 'string' && typeof raw !== 'number') return null;
  const s = String(raw).trim();
  if (!s) return null;
  const d = new Date(s);
  if (Number.isNaN(d.getTime())) return null;
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, '0');
  const day = String(d.getDate()).padStart(2, '0');
  const hh = String(d.getHours()).padStart(2, '0');
  const mm = String(d.getMinutes()).padStart(2, '0');
  return `${y}-${m}-${day} ${hh}:${mm}`;
}

function asFiniteNumber(raw: unknown): number | null {
  if (typeof raw === 'number' && Number.isFinite(raw)) return raw;
  if (typeof raw === 'string' && raw.trim() !== '') {
    const n = Number(raw);
    if (Number.isFinite(n)) return n;
  }
  return null;
}

function formatQuotaNumber(n: number): string {
  if (Number.isInteger(n)) return n.toLocaleString();
  return n.toLocaleString(undefined, { maximumFractionDigits: 2 });
}

/**
 * Human-readable quota / usage for a key row.
 * Prefers used/total when both exist; otherwise remain, then quota alone; unlimited last.
 */
export function formatKeyQuota(
  key: Pick<
    Sub2ApiKey,
    'quota' | 'used_quota' | 'remain_quota' | 'unlimited_quota'
  >,
  labels: { unlimited: string },
): string | null {
  if (key.unlimited_quota === true) return labels.unlimited;

  const used = asFiniteNumber(key.used_quota);
  const quota = asFiniteNumber(key.quota);
  const remain = asFiniteNumber(key.remain_quota);

  if (used != null && quota != null) {
    return `${formatQuotaNumber(used)} / ${formatQuotaNumber(quota)}`;
  }
  if (remain != null && quota != null) {
    return `${formatQuotaNumber(remain)} / ${formatQuotaNumber(quota)}`;
  }
  if (remain != null) return formatQuotaNumber(remain);
  if (quota != null) return formatQuotaNumber(quota);
  if (used != null) return formatQuotaNumber(used);
  return null;
}

/** Join model list; truncate long lists. Empty when missing. */
export function formatKeyModels(raw: unknown, maxItems = 6): string | null {
  if (raw == null) return null;
  let items: string[] = [];
  if (Array.isArray(raw)) {
    items = raw.map((x) => String(x ?? '').trim()).filter(Boolean);
  } else if (typeof raw === 'string') {
    const s = raw.trim();
    if (!s) return null;
    items = s.includes(',')
      ? s.split(',').map((p) => p.trim()).filter(Boolean)
      : [s];
  } else {
    return null;
  }
  if (items.length === 0) return null;
  if (items.length <= maxItems) return items.join(', ');
  const shown = items.slice(0, maxItems).join(', ');
  return `${shown} (+${items.length - maxItems})`;
}

/** Prefer group_name, then string `group`, then numeric group_id. */
export function pickGroupLabel(
  key: Pick<Sub2ApiKey, 'group_id' | 'group_name' | 'group'>,
): string | null {
  const name = typeof key.group_name === 'string' ? key.group_name.trim() : '';
  if (name) return name;
  if (typeof key.group === 'string') {
    const g = key.group.trim();
    if (g) return g;
  }
  if (key.group_id != null && Number.isFinite(Number(key.group_id))) {
    return String(key.group_id);
  }
  return null;
}
