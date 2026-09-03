import { describe, expect, it } from 'vitest';

import type { UsageTrendPoint } from '@/lib/types';

import {
  costFromTrendPoint,
  fmtTrendCost,
  isTrendCostKey,
  listTrendSeriesKeys,
  modelSeriesColor,
  rankTrendSeriesKeys,
  trendCostKey,
} from './usageTrendChartModel';

const points: UsageTrendPoint[] = [
  { date: '2026-08-26', opus: 100, sonnet: 40, '__cost__:opus': 1.5, '__cost__:sonnet': 0.2 },
  { date: '2026-08-27', opus: 20, sonnet: 80, '__cost__:opus': 0.25, '__cost__:sonnet': 0.8 },
];

describe('trend cost keys', () => {
  it('prefixes series names without colliding with date', () => {
    expect(trendCostKey('opus')).toBe('__cost__:opus');
    expect(isTrendCostKey('__cost__:opus')).toBe(true);
    expect(isTrendCostKey('opus')).toBe(false);
    expect(isTrendCostKey('date')).toBe(false);
  });
});

describe('list / rank model trend', () => {
  it('lists model keys and ranks by token totals', () => {
    const keys = listTrendSeriesKeys(points);
    expect(keys.sort()).toEqual(['opus', 'sonnet']);
    expect(rankTrendSeriesKeys(points, keys)).toEqual(['opus', 'sonnet']);
  });

  it('reads cost from the parallel key and formats like the table', () => {
    expect(costFromTrendPoint(points[0], 'opus')).toBe(1.5);
    expect(costFromTrendPoint(undefined, 'opus')).toBe(0);
    expect(fmtTrendCost(1.5)).toBe('$1.50');
  });

  it('cycles a stable palette', () => {
    expect(modelSeriesColor(0)).toBe(modelSeriesColor(12));
    expect(modelSeriesColor(0)).not.toBe(modelSeriesColor(1));
  });
});
