/** Pure helpers for the Sub2API routes page. */

import type { Sub2ApiKey, Sub2ApiSession, Sub2ApiUser } from '@/lib/sub2api';
import {
  SUB2API_DEFAULT_SITE_URL,
  normalizeSiteUrl,
  tryNormalizeSiteUrl,
  type NormalizeSiteUrlResult,
} from '@/lib/sub2api';

export type Sub2ApiPagePhase = 'restoring' | 'logged-out' | 'awaiting-2fa' | 'logged-in';

export type Sub2ApiKeyStatusKind =
  | 'active'
  | 'disabled'
  | 'expired'
  | 'quota_exhausted'
  | 'other';

export function sub2apiPagePhase(
  session: Sub2ApiSession | null,
  awaiting2fa: boolean,
  restoring = false,
): Sub2ApiPagePhase {
  if (restoring) return 'restoring';
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
  const raw = String(status ?? '')
    .trim()
    .toLowerCase()
    .replace(/[\s-]+/g, '_');
  if (raw === 'active' || raw === 'enabled' || raw === '1' || raw === 'ok') return 'active';
  if (
    raw === 'quota_exhausted'
    || raw === 'quotaexhausted'
    || raw === 'exhausted'
    || raw === 'no_quota'
  ) {
    return 'quota_exhausted';
  }
  if (raw === 'expired' || raw === 'expire') return 'expired';
  if (
    raw === 'disabled'
    || raw === 'inactive'
    || raw === '0'
    || raw === '2'
    || raw === 'banned'
  ) {
    return 'disabled';
  }
  return 'other';
}

export function sub2apiKeyStatusLabel(
  status: unknown,
  labels: {
    active: string;
    disabled: string;
    other: string;
    expired?: string;
    quotaExhausted?: string;
  },
): string {
  const kind = sub2apiKeyStatusKind(status);
  if (kind === 'active') return labels.active;
  if (kind === 'quota_exhausted') {
    return labels.quotaExhausted?.trim() || labels.disabled;
  }
  if (kind === 'expired') {
    return labels.expired?.trim() || labels.disabled;
  }
  if (kind === 'disabled') return labels.disabled;
  return labels.other;
}

export function sub2apiKeyStatusBadgeVariant(
  kind: Sub2ApiKeyStatusKind,
): 'success' | 'warning' | 'danger' | 'default' {
  switch (kind) {
    case 'active':
      return 'success';
    case 'expired':
      return 'warning';
    case 'quota_exhausted':
      return 'danger';
    case 'disabled':
      return 'warning';
    default:
      return 'default';
  }
}

export function initialSiteUrlDraft(session: Sub2ApiSession | null): string {
  return session?.siteUrl || SUB2API_DEFAULT_SITE_URL;
}

export function prepareSiteUrlForLogin(raw: string): string {
  return normalizeSiteUrl(raw || SUB2API_DEFAULT_SITE_URL);
}

/**
 * Blur/paste helper: normalize to origin, report strip/invalid for UX.
 * Empty input returns null (caller keeps draft / placeholder).
 */
export function applySiteUrlDraftInput(raw: string): {
  draft: string | null;
  result: NormalizeSiteUrlResult | null;
} {
  const trimmed = raw.trim();
  if (!trimmed) return { draft: null, result: null };
  const result = tryNormalizeSiteUrl(trimmed);
  if (!result.ok) return { draft: raw, result };
  return { draft: result.url, result };
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
  key: Sub2ApiKey,
  labels: { unlimited: string },
): string | null {
  const record = key as unknown as Record<string, unknown>;
  if (record.unlimited_quota === true) return labels.unlimited;

  const used = asFiniteNumber(record.quota_used ?? record.used_quota ?? record.used);
  const quota = asFiniteNumber(record.quota);
  const remain = asFiniteNumber(record.remain_quota ?? record.remaining);

  if (quota === 0 && used == null && remain == null) return labels.unlimited;
  if (used != null && quota != null && quota > 0) {
    return `${formatQuotaNumber(used)} / ${formatQuotaNumber(quota)}`;
  }
  if (remain != null && quota != null && quota > 0) {
    return `${formatQuotaNumber(remain)} / ${formatQuotaNumber(quota)}`;
  }
  if (quota === 0 && used != null) return formatQuotaNumber(used);
  if (remain != null) return formatQuotaNumber(remain);
  if (quota != null && quota > 0) return formatQuotaNumber(quota);
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

/** Prefer group_name, then embedded group.name / string group, then numeric group_id. */
export function pickGroupLabel(key: Sub2ApiKey): string | null {
  const record = key as unknown as Record<string, unknown>;
  const named = typeof record.group_name === 'string' ? record.group_name.trim() : '';
  if (named) return named;
  const group = record.group;
  if (typeof group === 'string') {
    const g = group.trim();
    if (g) return g;
  }
  if (group && typeof group === 'object') {
    const gName = (group as Record<string, unknown>).name;
    if (typeof gName === 'string' && gName.trim()) return gName.trim();
  }
  if (record.group_id != null && Number.isFinite(Number(record.group_id))) {
    return String(record.group_id);
  }
  return null;
}

/** Models from the key itself or nested group config when present. */
export function formatKeyModelsFromKey(key: Sub2ApiKey, maxItems = 6): string | null {
  const record = key as unknown as Record<string, unknown>;
  const direct = formatKeyModels(record.models ?? record.model_list ?? record.allowed_models, maxItems);
  if (direct) return direct;
  const group = record.group;
  if (group && typeof group === 'object') {
    const g = group as Record<string, unknown>;
    const cfg = g.models_list_config ?? g.modelsListConfig;
    const cfgObj = cfg && typeof cfg === 'object' ? (cfg as Record<string, unknown>) : null;
    const cfgModels = cfgObj && cfgObj.enabled !== false ? cfgObj.models : undefined;
    const nested = g.models ?? cfgModels;
    return formatKeyModels(nested, maxItems);
  }
  return null;
}
