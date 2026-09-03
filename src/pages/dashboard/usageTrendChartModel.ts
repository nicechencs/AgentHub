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
