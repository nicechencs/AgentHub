/**
 * Usage sync helpers — interval scheduling + status copy.
 * Actual timers live in UsageSyncProvider; this module stays pure/testable.
 */

import { loadString, saveString, StorageKey } from '@/lib/ui-preferences';

/** Dispatched after settings save so the provider reloads interval. */
export const USAGE_SYNC_SETTINGS_CHANGED = 'agenthub:usage-sync-settings';

/** Dispatched after a successful collect (manual or auto). */
export const USAGE_COLLECTED_EVENT = 'agenthub:usage-collected';

export type UsageCollectSource = 'manual' | 'auto';

export interface UsageCollectedDetail {
  source: UsageCollectSource;
  inserted?: number;
  at: number;
}

export function loadLastCollectAt(now = Date.now()): number | null {
  const raw = loadString(StorageKey.usageLastCollectAt, '');
  if (!raw) return null;
  const n = Number(raw);
  if (!Number.isFinite(n) || n <= 0 || n > now + 60_000) return null;
  return n;
}

export function saveLastCollectAt(at: number): void {
  saveString(StorageKey.usageLastCollectAt, String(at));
}

/** Normalize interval minutes: 0 = manual only; clamp wild values. */
export function normalizeIntervalMin(raw: unknown): number {
  const n = typeof raw === 'number' ? raw : Number(raw);
  if (!Number.isFinite(n) || n <= 0) return 0;
  return Math.min(24 * 60, Math.floor(n));
}

/** Automatic collection retry policy. Keep transient failures away from the
 * overdue grace timer, which is intentionally short for a healthy schedule. */
export const AUTO_RETRY_BASE_MS = 30_000;
export const AUTO_RETRY_MAX_MS = 15 * 60_000;

export function computeAutoRetryDelay(failureCount: number, intervalMin: number): number {
  const count = Math.max(1, Math.floor(Number.isFinite(failureCount) ? failureCount : 1));
  const exponential = Math.min(AUTO_RETRY_MAX_MS, AUTO_RETRY_BASE_MS * 2 ** (count - 1));
  const normalInterval = normalizeIntervalMin(intervalMin) * 60_000;
  return normalInterval > 0 ? Math.min(exponential, normalInterval) : exponential;
}

export function computeAutoRetryAt(
  lastAttemptAt: number,
  intervalMin: number,
  failureCount: number,
  now = Date.now(),
): number | null {
  if (normalizeIntervalMin(intervalMin) <= 0) return null;
  return Math.max(now, lastAttemptAt + computeAutoRetryDelay(failureCount, intervalMin));
}

/**
 * Next fire time from last collect + interval.
 * - interval 0 → null (manual only)
 * - no last → now + interval (avoid surprise collect on every cold start)
 * - overdue → now (caller may add a short grace delay)
 */
export function computeNextCollectAt(
  lastCollectAt: number | null,
  intervalMin: number,
  now = Date.now(),
): number | null {
  const mins = normalizeIntervalMin(intervalMin);
  if (mins <= 0) return null;
  const ms = mins * 60_000;
  if (lastCollectAt == null) return now + ms;
  return Math.max(now, lastCollectAt + ms);
}

export function formatDurationShort(ms: number): string {
  const totalSec = Math.max(0, Math.ceil(ms / 1000));
  if (totalSec < 60) return `${totalSec} 秒`;
  const totalMin = Math.floor(totalSec / 60);
  const sec = totalSec % 60;
  if (totalMin < 60) {
    return sec > 0 ? `${totalMin} 分 ${sec} 秒` : `${totalMin} 分钟`;
  }
  const hours = Math.floor(totalMin / 60);
  const min = totalMin % 60;
  return min > 0 ? `${hours} 小时 ${min} 分` : `${hours} 小时`;
}

export function formatLastCollectLabel(lastCollectAt: number | null, now = Date.now()): string {
  if (lastCollectAt == null) return '尚未同步';
  const ago = Math.max(0, now - lastCollectAt);
  if (ago < 15_000) return '上次同步：刚刚';
  return `上次同步：${formatDurationShort(ago)}前`;
}

export function formatNextCollectLabel(
  nextCollectAt: number | null,
  intervalMin: number,
  now = Date.now(),
): string {
  const mins = normalizeIntervalMin(intervalMin);
  if (mins <= 0) return '仅手动采集';
  if (nextCollectAt == null) return `每 ${mins} 分钟自动同步`;
  const remain = nextCollectAt - now;
  if (remain <= 0) return '即将自动同步';
  return `还有 ${formatDurationShort(remain)} 自动同步`;
}

/** Status line for Dashboard: countdown / manual only (no last-sync time). */
export function buildUsageSyncStatusLine(opts: {
  /** Kept for callers; last-sync is intentionally not shown in UI. */
  lastCollectAt: number | null;
  nextCollectAt: number | null;
  intervalMin: number;
  collecting: boolean;
  now?: number;
}): string {
  const now = opts.now ?? Date.now();
  if (opts.collecting) return '正在同步用量…';
  return formatNextCollectLabel(opts.nextCollectAt, opts.intervalMin, now);
}

export function notifyUsageSettingsChanged(): void {
  if (typeof window === 'undefined') return;
  window.dispatchEvent(new Event(USAGE_SYNC_SETTINGS_CHANGED));
}

export function notifyUsageCollected(detail: UsageCollectedDetail): void {
  if (typeof window === 'undefined') return;
  window.dispatchEvent(new CustomEvent(USAGE_COLLECTED_EVENT, { detail }));
}
