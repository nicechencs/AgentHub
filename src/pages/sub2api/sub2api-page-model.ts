/** Pure helpers for the Sub2API routes page. */

import type { Sub2ApiGroup, Sub2ApiKey, Sub2ApiSession, Sub2ApiUser } from '@/lib/sub2api';
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

/** Sub2API toggle: active → inactive, anything else → active. */
export function nextSub2ApiKeyToggleStatus(status: unknown): 'active' | 'inactive' {
  return sub2apiKeyStatusKind(status) === 'active' ? 'inactive' : 'active';
}

export function mergeUpdatedSub2ApiKey(
  prev: Sub2ApiKey,
  updated: Sub2ApiKey | null | undefined,
): Sub2ApiKey {
  if (!updated || typeof updated !== 'object') return prev;
  const key = typeof updated.key === 'string' && updated.key.trim() ? updated.key : prev.key;
  return { ...prev, ...updated, id: prev.id, key };
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
    const ad = parseKeyDate(a.created_at)?.getTime() ?? 0;
    const bd = parseKeyDate(b.created_at)?.getTime() ?? 0;
    if (ad !== bd) return bd - ad;
    return b.id - a.id;
  });
}

/** 6-digit TOTP only — strip non-digits and cap length. */
export function normalizeTotpCode(raw: string): string {
  return raw.replace(/\D/g, '').slice(0, 6);
}

/** Parse ISO / unix seconds / unix ms into a local Date. */
export function parseKeyDate(raw: unknown): Date | null {
  if (raw == null) return null;
  if (typeof raw === 'number' && Number.isFinite(raw)) {
    if (raw <= 0) return null;
    const ms = raw < 1e12 ? raw * 1000 : raw;
    const d = new Date(ms);
    return Number.isNaN(d.getTime()) ? null : d;
  }
  if (typeof raw !== 'string') return null;
  const s = raw.trim();
  if (!s) return null;
  if (/^-?\d+(\.\d+)?$/.test(s)) return parseKeyDate(Number(s));
  const d = new Date(s);
  return Number.isNaN(d.getTime()) ? null : d;
}

function pad2(n: number): string {
  return String(n).padStart(2, '0');
}

function formatLocalDateParts(d: Date, withSeconds: boolean, sep: '-' | '/'): string {
  const y = d.getFullYear();
  const m = pad2(d.getMonth() + 1);
  const day = pad2(d.getDate());
  const hh = pad2(d.getHours());
  const mm = pad2(d.getMinutes());
  if (!withSeconds) return `${y}${sep}${m}${sep}${day} ${hh}:${mm}`;
  return `${y}${sep}${m}${sep}${day} ${hh}:${mm}:${pad2(d.getSeconds())}`;
}

/** Format ISO / date-like strings as `YYYY-MM-DD HH:mm` (local). Empty when missing/invalid. */
export function formatKeyTimestamp(raw: unknown): string | null {
  const d = parseKeyDate(raw);
  if (!d) return null;
  return formatLocalDateParts(d, false, '-');
}

/** Sub2API site table: `YYYY/MM/DD HH:mm:ss` (local). */
export function formatKeyTableTimestamp(raw: unknown): string | null {
  const d = parseKeyDate(raw);
  if (!d) return null;
  return formatLocalDateParts(d, true, '/');
}

export function formatKeyExpires(raw: unknown, neverLabel: string): string {
  if (raw == null) return neverLabel;
  if (typeof raw === 'number' && raw <= 0) return neverLabel;
  if (typeof raw === 'string') {
    const s = raw.trim().toLowerCase();
    if (!s || s === 'null' || s === 'never' || s === '0' || s === '-1') return neverLabel;
  }
  return formatKeyTableTimestamp(raw) ?? neverLabel;
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

/** Numeric group id from the key or nested group object. */
export function pickGroupId(key: Sub2ApiKey): number | null {
  const record = key as unknown as Record<string, unknown>;
  if (record.group_id != null && Number.isFinite(Number(record.group_id))) {
    return Number(record.group_id);
  }
  const group = record.group;
  if (group && typeof group === 'object') {
    const id = (group as Record<string, unknown>).id;
    if (id != null && Number.isFinite(Number(id))) return Number(id);
  }
  return null;
}

export function applyGroupToKey(key: Sub2ApiKey, group: Sub2ApiGroup | null): Sub2ApiKey {
  if (!group) {
    return { ...key, group_id: null, group_name: null, group: null };
  }
  const prev = typeof key.group === 'object' && key.group ? key.group : {};
  return {
    ...key,
    group_id: group.id,
    group_name: group.name,
    group: {
      ...prev,
      id: group.id,
      name: group.name,
      ...(group.platform ? { platform: group.platform } : {}),
      ...(typeof group.rate_multiplier === 'number' && Number.isFinite(group.rate_multiplier)
        ? { rate_multiplier: group.rate_multiplier }
        : {}),
    },
  };
}

export type Sub2ApiGroupFilter = 'all' | 'none' | number;

export function parseGroupFilter(raw: string): Sub2ApiGroupFilter {
  if (raw === 'none') return 'none';
  if (raw === 'all' || raw === '') return 'all';
  const n = Number(raw);
  return Number.isFinite(n) ? n : 'all';
}

export function keyMatchesGroupFilter(key: Sub2ApiKey, filter: Sub2ApiGroupFilter): boolean {
  if (filter === 'all') return true;
  const id = pickGroupId(key);
  if (filter === 'none') return id == null;
  return id === filter;
}

/** Merge /groups/available with groups already present on keys. */
export function mergeSub2ApiGroups(
  available: readonly Sub2ApiGroup[],
  keys: readonly Sub2ApiKey[],
): Sub2ApiGroup[] {
  const byId = new Map<number, Sub2ApiGroup>();
  for (const group of available) {
    if (!Number.isFinite(group.id)) continue;
    const name = group.name?.trim() || String(group.id);
    byId.set(group.id, { ...group, name });
  }
  for (const key of keys) {
    const id = pickGroupId(key);
    if (id == null || byId.has(id)) continue;
    const label = pickGroupLabel(key);
    byId.set(id, { id, name: label && label !== String(id) ? label : String(id) });
  }
  return [...byId.values()].sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true }));
}

export function pickGroupPlatform(
  key: Sub2ApiKey,
  groups: readonly Sub2ApiGroup[] = [],
): string | null {
  const group = (key as unknown as Record<string, unknown>).group;
  if (group && typeof group === 'object') {
    const platform = (group as Record<string, unknown>).platform;
    if (typeof platform === 'string' && platform.trim()) return platform.trim().toLowerCase();
  }
  const id = pickGroupId(key);
  if (id == null) return null;
  const found = groups.find((row) => row.id === id)?.platform?.trim();
  return found ? found.toLowerCase() : null;
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

function readNumber(record: Record<string, unknown>, paths: readonly string[]): number | null {
  for (const path of paths) {
    const parts = path.split('.');
    let cur: unknown = record;
    for (const part of parts) {
      if (!cur || typeof cur !== 'object') {
        cur = undefined;
        break;
      }
      cur = (cur as Record<string, unknown>)[part];
    }
    const n = asFiniteNumber(cur);
    if (n != null) return n;
  }
  return null;
}

const CONCURRENCY_PATHS = [
  'current_concurrency',
  'concurrency',
  'concurrent',
  'concurrent_count',
  'current_concurrent',
  'currentConcurrent',
] as const;

const USAGE_TODAY_PATHS = [
  'today_usage',
  'today_consumed',
  'today_cost',
  'today_spent',
  'usage_today',
  'quota_today',
  'used_today',
  'today_amount',
  'daily_usage',
  'usage.today',
  'usage.today_usd',
  'stats.today',
  'stats.today_usd',
] as const;

const USAGE_LAST_30_PATHS = [
  'last_30_days_usage',
  'last_30d_usage',
  'usage_30d',
  'last30days_usage',
  'last_30_days_consumed',
  'last_30_days_cost',
  'month_usage',
  'monthly_usage',
  'cost_30d',
  'usage.last_30_days',
  'usage.last30Days',
  'usage.last_30_days_usd',
  'stats.last_30_days',
] as const;

const GROUP_RATE_KEYS = [
  'rate',
  'ratio',
  'multiplier',
  'rate_multiplier',
  'billing_rate',
  'price_rate',
  'price_ratio',
] as const;

export function pickKeyConcurrency(key: Sub2ApiKey): number {
  const n = readNumber(key as unknown as Record<string, unknown>, CONCURRENCY_PATHS);
  return n == null ? 0 : n;
}

export function pickKeyUsageUsd(key: Sub2ApiKey): { today: number; last30Days: number } {
  const record = key as unknown as Record<string, unknown>;
  return {
    today: readNumber(record, USAGE_TODAY_PATHS) ?? 0,
    last30Days: readNumber(record, USAGE_LAST_30_PATHS) ?? 0,
  };
}

export function formatUsdAmount(n: number): string {
  return `$${n.toFixed(4)}`;
}

export function formatGroupRate(n: number): string {
  const s = Number.isInteger(n)
    ? String(n)
    : n.toFixed(2).replace(/0+$/, '').replace(/\.$/, '');
  return `${s}x`;
}

export function pickGroupRate(key: Sub2ApiKey): string | null {
  const group = (key as unknown as Record<string, unknown>).group;
  if (!group || typeof group !== 'object') return null;
  const g = group as Record<string, unknown>;
  for (const k of GROUP_RATE_KEYS) {
    const n = asFiniteNumber(g[k]);
    if (n != null) return formatGroupRate(n);
  }
  return null;
}

/** Site table mask: `sk-c33...62e2`. */
export function maskSub2ApiTableKey(key: string): string {
  const value = key.trim();
  if (!value) return '••••';
  if (value.length <= 10) return `${value.slice(0, 2)}...${value.slice(-2)}`;
  return `${value.slice(0, 6)}...${value.slice(-4)}`;
}
