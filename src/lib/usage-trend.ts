import type { UsageTrendPoint } from '@/lib/types';

/** `days <= 1` (today / last 24h) is hourly; longer windows stay daily. */
export type TrendGrain = 'hour' | 'day';

const HOURLY_BUCKET = /^(\d{4})-(\d{2})-(\d{2}) (\d{2}):00$/;

export function trendGrain(days: number): TrendGrain {
  return days <= 1 ? 'hour' : 'day';
}

function pad2(n: number): string {
  return String(n).padStart(2, '0');
}

function formatLocalBucket(d: Date, grain: TrendGrain): string {
  const day = `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`;
  return grain === 'day' ? day : `${day} ${pad2(d.getHours())}:00`;
}

/** Local hour (`YYYY-MM-DD HH:00`) or calendar day (`YYYY-MM-DD`). */
export function localTrendBucket(iso: string, grain: TrendGrain): string | null {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return null;
  return formatLocalBucket(d, grain);
}

/** Dense x-axis keys for the selected window (local time). */
export function denseTrendBuckets(days: number, since?: string, now = new Date()): string[] {
  const grain = trendGrain(days);
  const windowDays = Math.max(1, days);
  const rollingStart = new Date(now.getTime() - windowDays * 24 * 3600 * 1000);
  let start = rollingStart;
  if (since) {
    const bound = new Date(since);
    if (!Number.isNaN(bound.getTime()) && bound > start) start = bound;
  }

  const keys: string[] = [];
  if (grain === 'hour') {
    const t = new Date(start);
    t.setMinutes(0, 0, 0);
    const end = new Date(now);
    end.setMinutes(0, 0, 0);
    while (t <= end && keys.length < 48) {
      keys.push(formatLocalBucket(t, 'hour'));
      t.setHours(t.getHours() + 1);
    }
  } else {
    const t = new Date(start.getFullYear(), start.getMonth(), start.getDate());
    const end = new Date(now.getFullYear(), now.getMonth(), now.getDate());
    while (t <= end && keys.length < 40) {
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
