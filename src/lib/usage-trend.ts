import type { UsageTrendPoint } from '@/lib/types';

/** `days <= 1` (today / last 24h) is hourly; longer windows stay daily. */
export type TrendGrain = 'hour' | 'day';

const HOURLY_BUCKET = /^(\d{4})-(\d{2})-(\d{2}) (\d{2}):00$/;

export function trendGrain(days: number, since?: string, until?: string): TrendGrain {
  if (since && until) {
    const start = new Date(since).getTime();
    const end = new Date(until).getTime();
    if (Number.isFinite(start) && Number.isFinite(end)) {
      return end - start <= 24 * 3600 * 1000 ? 'hour' : 'day';
    }
  }
  return days <= 1 ? 'hour' : 'day';
}

function pad2(n: number): string {
  return String(n).padStart(2, '0');
}

function formatLocalBucket(d: Date, grain: TrendGrain): string {
  const day = `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`;
  return grain === 'day' ? day : `${day} ${pad2(d.getHours())}:00`;
}

/** Current local bucket for a Today marker (day or current hour). */
export function todayTrendBucket(now = new Date(), grain: TrendGrain = 'day'): string {
  return formatLocalBucket(now, grain);
}

export function trendPointGrain(date: string): TrendGrain {
  return HOURLY_BUCKET.test(date) ? 'hour' : 'day';
}

/** Local hour (`YYYY-MM-DD HH:00`) or calendar day (`YYYY-MM-DD`). */
export function localTrendBucket(iso: string, grain: TrendGrain): string | null {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return null;
  return formatLocalBucket(d, grain);
}

/** Dense x-axis keys for the selected window (local time). */
export function denseTrendBuckets(
  days: number,
  since?: string,
  now = new Date(),
  until?: string,
): string[] {
  const grain = trendGrain(days, since, until);
  const windowDays = Math.max(1, days);
  const rollingStart = new Date(now.getTime() - windowDays * 24 * 3600 * 1000);
  let start = rollingStart;
  if (since) {
    const bound = new Date(since);
    if (!Number.isNaN(bound.getTime()) && bound > start) start = bound;
  }
  let endAt = now;
  if (until) {
    const bound = new Date(until);
    if (!Number.isNaN(bound.getTime())) {
      endAt = new Date(bound.getTime() - 1);
      if (endAt > now) endAt = now;
    }
  }

  const keys: string[] = [];
  if (grain === 'hour') {
    const t = new Date(start);
    t.setMinutes(0, 0, 0);
    const end = new Date(endAt);
    end.setMinutes(0, 0, 0);
    while (t <= end && keys.length < 48) {
      keys.push(formatLocalBucket(t, 'hour'));
      t.setHours(t.getHours() + 1);
    }
  } else {
    const t = new Date(start.getFullYear(), start.getMonth(), start.getDate());
    const end = new Date(endAt.getFullYear(), endAt.getMonth(), endAt.getDate());
    while (t <= end && keys.length < 100) {
      keys.push(formatLocalBucket(t, 'day'));
      t.setDate(t.getDate() + 1);
    }
  }
  return keys;
}

/** Axis tick: daily `MM-DD`; hourly `HH:00`, midnight shows the date. */
export function formatTrendTick(date: string): string {
  const m = HOURLY_BUCKET.exec(date);
  if (m) {
    return m[4] === '00' ? `${m[2]}-${m[3]}` : `${m[4]}:00`;
  }
  return date.length >= 10 ? date.slice(5, 10) : date;
}

/** Tooltip title: keep the day visible on hourly points. */
export function formatTrendTooltipLabel(date: string): string {
  const m = HOURLY_BUCKET.exec(date);
  if (m) return `${m[2]}-${m[3]} ${m[4]}:00`;
  return date;
}

export interface UsageTrendTooltipItem {
  key: string;
  name: string;
  tokens: number;
  formatted?: string;
  extra?: string;
  share?: string;
  color?: string;
}

export interface UsageTrendTooltipPayloadEntry {
  value?: unknown;
  name?: unknown;
  color?: string;
  dataKey?: unknown;
  payload?: Record<string, unknown>;
}

/** Drop empty series, then highest token usage first. */
export function sortUsageTrendTooltipItems(
  items: readonly UsageTrendTooltipItem[],
): UsageTrendTooltipItem[] {
  return items
    .filter((item) => Number.isFinite(item.tokens) && item.tokens > 0)
    .slice()
    .sort(
      (a, b) => b.tokens - a.tokens || a.name.localeCompare(b.name) || a.key.localeCompare(b.key),
    );
}

export function usageTrendTooltipItemsFromPayload(
  payload: readonly UsageTrendTooltipPayloadEntry[] | null | undefined,
  resolveName?: (key: string) => string,
): UsageTrendTooltipItem[] {
  return sortUsageTrendTooltipItems(
    (payload ?? []).map((entry) => {
      const key = String(entry.dataKey ?? entry.name ?? '');
      return {
        key,
        name: resolveName?.(key) ?? String(entry.name ?? key),
        tokens: Number(entry.value) || 0,
        color: typeof entry.color === 'string' ? entry.color : undefined,
      };
    }),
  );
}

/** Missing series keys become 0 so stacked areas do not gap. */
export function zeroFillTrendSeries(
  points: readonly UsageTrendPoint[],
  agentIds: readonly string[],
): UsageTrendPoint[] {
  return points.map((point) => {
    const next: UsageTrendPoint = { ...point };
    for (const id of agentIds) {
      if (typeof next[id] !== 'number') next[id] = 0;
    }
    return next;
  });
}
