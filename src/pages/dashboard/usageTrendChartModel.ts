import type { UsageTrendPoint } from '@/lib/types';

/** Must match core `USAGE_TREND_COST_KEY_PREFIX`. */
export const TREND_COST_KEY_PREFIX = '__cost__:';

export type UsageTrendGroup = 'agent' | 'model';

export function isTrendCostKey(key: string): boolean {
  return key.startsWith(TREND_COST_KEY_PREFIX);
}

export function trendCostKey(seriesKey: string): string {
  return `${TREND_COST_KEY_PREFIX}${seriesKey}`;
}

export function listTrendSeriesKeys(points: readonly UsageTrendPoint[]): string[] {
  const keys = new Set<string>();
  for (const point of points) {
    for (const key of Object.keys(point)) {
      if (key === 'date' || isTrendCostKey(key)) continue;
      keys.add(key);
    }
  }
  return [...keys];
}

export function rankTrendSeriesKeys(
  points: readonly UsageTrendPoint[],
  keys: readonly string[],
): string[] {
  const totals = new Map<string, number>();
  for (const key of keys) totals.set(key, 0);
  for (const point of points) {
    for (const key of keys) {
      totals.set(key, (totals.get(key) ?? 0) + (Number(point[key]) || 0));
    }
  }
  return [...keys].sort((a, b) => {
    const delta = (totals.get(b) ?? 0) - (totals.get(a) ?? 0);
    if (delta !== 0) return delta;
    return a.localeCompare(b);
  });
}

export function fmtTrendCost(n: number): string {
  return `$${n.toFixed(2)}`;
}

export function costFromTrendPoint(
  point: UsageTrendPoint | undefined,
  seriesKey: string,
): number {
  if (!point) return 0;
  return Number(point[trendCostKey(seriesKey)]) || 0;
}

const MODEL_SERIES_COLORS = [
  '#4f46e5',
  '#d97757',
  '#0f766e',
  '#2563eb',
  '#c2410c',
  '#e11d48',
  '#16a34a',
  '#7c3aed',
  '#0891b2',
  '#ca8a04',
  '#64748b',
  '#db2777',
] as const;

export function modelSeriesColor(index: number): string {
  return MODEL_SERIES_COLORS[index % MODEL_SERIES_COLORS.length]!;
}

/** Long-tail models fold into this series when grouping by model. */
export const MODEL_OTHER_KEY = '__other__';

/** Named model layers before the remainder becomes Other. */
export const MODEL_NAMED_SERIES_LIMIT = 7;

export function isModelOtherKey(key: string): boolean {
  return key === MODEL_OTHER_KEY;
}

export function sumTrendSeriesTokens(
  point: UsageTrendPoint | undefined,
  keys: readonly string[],
): number {
  if (!point) return 0;
  let total = 0;
  for (const key of keys) total += Number(point[key]) || 0;
  return total;
}

export function trendSharePct(part: number, total: number): string {
  if (!(total > 0) || !Number.isFinite(part)) return '0%';
  return `${((part / total) * 100).toFixed(1)}%`;
}

function seriesTotal(points: readonly UsageTrendPoint[], key: string): number {
  let total = 0;
  for (const point of points) total += Number(point[key]) || 0;
  return total;
}

/** Keep the top models by tokens; fold the rest into Other (including cost). */
export function foldTrendTail(
  points: readonly UsageTrendPoint[],
  rankedKeys: readonly string[],
  limit = MODEL_NAMED_SERIES_LIMIT,
): { keys: string[]; points: UsageTrendPoint[] } {
  const withVolume = rankedKeys.filter((key) => seriesTotal(points, key) > 0);
  if (withVolume.length <= limit) {
    return {
      keys: withVolume,
      points: points.map((point) => cloneTrendPoint(point, withVolume)),
    };
  }
  const head = withVolume.slice(0, limit);
  const tail = withVolume.slice(limit);
  const keys = [...head, MODEL_OTHER_KEY];
  const folded = points.map((point) => {
    const next = cloneTrendPoint(point, head);
    let otherTokens = 0;
    let otherCost = 0;
    for (const key of tail) {
      otherTokens += Number(point[key]) || 0;
      otherCost += Number(point[trendCostKey(key)]) || 0;
    }
    next[MODEL_OTHER_KEY] = otherTokens;
    if (otherCost) next[trendCostKey(MODEL_OTHER_KEY)] = otherCost;
    return next;
  });
  return { keys, points: folded };
}

export function accumulateTrendSeries(
  points: readonly UsageTrendPoint[],
  keys: readonly string[],
): UsageTrendPoint[] {
  const running = new Map<string, number>();
  const runningCost = new Map<string, number>();
  for (const key of keys) {
    running.set(key, 0);
    runningCost.set(key, 0);
  }
  return points.map((point) => {
    const next: UsageTrendPoint = { date: point.date };
    for (const key of keys) {
      const tokens = (running.get(key) ?? 0) + (Number(point[key]) || 0);
      running.set(key, tokens);
      next[key] = tokens;
      const cost = (runningCost.get(key) ?? 0) + (Number(point[trendCostKey(key)]) || 0);
      runningCost.set(key, cost);
      if (cost) next[trendCostKey(key)] = cost;
    }
    return next;
  });
}

function cloneTrendPoint(point: UsageTrendPoint, keys: readonly string[]): UsageTrendPoint {
  const next: UsageTrendPoint = { date: point.date };
  for (const key of keys) {
    next[key] = Number(point[key]) || 0;
    const cost = Number(point[trendCostKey(key)]) || 0;
    if (cost) next[trendCostKey(key)] = cost;
  }
  return next;
}
