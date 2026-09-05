/** Create/edit form helpers aligned with Sub2API KeysView. */

import type { Sub2ApiKey, Sub2ApiKeyPatch } from '@/lib/sub2api';
import { parseKeyDate, pickGroupId, sub2apiKeyStatusKind } from './sub2api-page-model';

export type Sub2ApiKeyForm = {
  name: string;
  groupId: number | null;
  status: 'active' | 'inactive';
  enableIpRestriction: boolean;
  ipWhitelist: string;
  ipBlacklist: string;
  quota: string;
  enableRateLimit: boolean;
  rateLimit5h: string;
  rateLimit1d: string;
  rateLimit7d: string;
  enableExpiration: boolean;
  expirationPreset: '7' | '30' | '90' | 'custom';
  expirationDate: string;
};

function asFiniteNumber(raw: unknown): number | null {
  if (typeof raw === 'number' && Number.isFinite(raw)) return raw;
  if (typeof raw === 'string' && raw.trim()) {
    const n = Number(raw);
    if (Number.isFinite(n)) return n;
  }
  return null;
}

function asPositiveNumber(raw: unknown): number | null {
  const n = asFiniteNumber(raw);
  return n != null && n > 0 ? n : null;
}

export function parseIpList(text: string): string[] {
  return text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}

export function formatIpList(list: unknown): string {
  if (!Array.isArray(list)) return '';
  return list.map((item) => String(item).trim()).filter(Boolean).join('\n');
}

export function parseUsdField(raw: string): number {
  const n = Number(raw.trim());
  return Number.isFinite(n) && n > 0 ? n : 0;
}

export function formatUsdField(raw: unknown): string {
  const n = asPositiveNumber(raw);
  if (n == null) return '';
  return Number.isInteger(n) ? String(n) : String(n);
}

export function formatUsdFixed(n: number, digits: number): string {
  return `$${n.toFixed(digits)}`;
}

export function formatDateTimeLocal(raw: unknown): string {
  const d = raw instanceof Date ? raw : parseKeyDate(raw);
  if (!d) return '';
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export function expirationToIso(localValue: string): string | null {
  const trimmed = localValue.trim();
  if (!trimmed) return null;
  const d = new Date(trimmed);
  if (Number.isNaN(d.getTime())) return null;
  return d.toISOString();
}

export function addDaysToDateTimeLocal(days: number, from = new Date()): string {
  const d = new Date(from);
  d.setDate(d.getDate() + days);
  return formatDateTimeLocal(d);
}

export function shouldSubmitEditStatus(
  keyStatus: unknown,
  next: 'active' | 'inactive',
): boolean {
  const kind = sub2apiKeyStatusKind(keyStatus);
  if (kind === 'quota_exhausted' || kind === 'expired') return next === 'active';
  return true;
}

export function pickQuotaLimit(key: Sub2ApiKey): number {
  return asPositiveNumber(key.quota) ?? 0;
}

export function pickQuotaUsed(key: Sub2ApiKey): number {
  return asFiniteNumber(key.quota_used) ?? asFiniteNumber(key.used_quota) ?? 0;
}

export function pickRateWindow(
  key: Sub2ApiKey,
  window: '5h' | '1d' | '7d',
): { limit: number; used: number } {
  const record = key as unknown as Record<string, unknown>;
  return {
    limit: asPositiveNumber(record[`rate_limit_${window}`]) ?? 0,
    used: asFiniteNumber(record[`usage_${window}`]) ?? 0,
  };
}

export function rateUsagePercent(used: number, limit: number): number {
  if (limit <= 0) return 0;
  return Math.min((used / limit) * 100, 100);
}

export function rateUsageTone(used: number, limit: number): 'ok' | 'warn' | 'over' {
  if (limit <= 0) return 'ok';
  if (used >= limit) return 'over';
  if (used >= limit * 0.8) return 'warn';
  return 'ok';
}

function editStatusFromKey(key: Sub2ApiKey): 'active' | 'inactive' {
  const kind = sub2apiKeyStatusKind(key.status);
  if (kind === 'quota_exhausted' || kind === 'expired' || kind === 'disabled') return 'inactive';
  return 'active';
}

export function formFromKey(key: Sub2ApiKey): Sub2ApiKeyForm {
  const whitelist = formatIpList(key.ip_whitelist);
  const blacklist = formatIpList(key.ip_blacklist);
  const r5 = formatUsdField(key.rate_limit_5h);
  const r1 = formatUsdField(key.rate_limit_1d);
  const r7 = formatUsdField(key.rate_limit_7d);
  const expires = formatDateTimeLocal(key.expires_at);
  return {
    name: key.name || '',
    groupId: pickGroupId(key),
    status: editStatusFromKey(key),
    enableIpRestriction: Boolean(whitelist || blacklist),
    ipWhitelist: whitelist,
    ipBlacklist: blacklist,
    quota: formatUsdField(key.quota),
    enableRateLimit: Boolean(r5 || r1 || r7),
    rateLimit5h: r5,
    rateLimit1d: r1,
    rateLimit7d: r7,
    enableExpiration: Boolean(expires),
    expirationPreset: 'custom',
    expirationDate: expires,
  };
}

export function buildEditPatch(form: Sub2ApiKeyForm, key: Sub2ApiKey): Sub2ApiKeyPatch {
  const iso = expirationToIso(form.expirationDate);
  const patch: Sub2ApiKeyPatch = {
    name: form.name.trim() || key.name || 'AgentHub',
    group_id: form.groupId,
    ip_whitelist: form.enableIpRestriction ? parseIpList(form.ipWhitelist) : [],
    ip_blacklist: form.enableIpRestriction ? parseIpList(form.ipBlacklist) : [],
    quota: parseUsdField(form.quota),
    expires_at: form.enableExpiration && iso ? iso : '',
    rate_limit_5h: form.enableRateLimit ? parseUsdField(form.rateLimit5h) : 0,
    rate_limit_1d: form.enableRateLimit ? parseUsdField(form.rateLimit1d) : 0,
    rate_limit_7d: form.enableRateLimit ? parseUsdField(form.rateLimit7d) : 0,
  };
  if (shouldSubmitEditStatus(key.status, form.status)) patch.status = form.status;
  return patch;
}
