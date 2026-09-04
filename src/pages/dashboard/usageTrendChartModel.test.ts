import { describe, expect, it } from 'vitest';

import type { UsageTrendPoint } from '@/lib/types';

import {
  accumulateTrendSeries,
  costFromTrendPoint,
  foldTrendTail,
  fmtTrendCost,
  isTrendCostKey,
  listTrendSeriesKeys,
  MODEL_NAMED_SERIES_LIMIT,
  MODEL_OTHER_KEY,
  modelSeriesColor,
  rankTrendSeriesKeys,
  sumTrendSeriesTokens,
  trendCostKey,
  trendSharePct,
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

describe('foldTrendTail / accumulateTrendSeries', () => {
  it('keeps seven named models and folds the rest into Other with costs', () => {
    const keys = Array.from({ length: MODEL_NAMED_SERIES_LIMIT + 2 }, (_, i) => `m${i}`);
    const point: UsageTrendPoint = { date: '2026-09-02' };
    for (const [index, key] of keys.entries()) {
      point[key] = (keys.length - index) * 10;
      point[trendCostKey(key)] = (keys.length - index) * 0.1;
    }
    const folded = foldTrendTail([point], keys);
    expect(folded.keys).toEqual([...keys.slice(0, 7), MODEL_OTHER_KEY]);
    expect(folded.points[0]?.m0).toBe(90);
    expect(folded.points[0]?.[MODEL_OTHER_KEY]).toBe(30);
    expect(folded.points[0]?.[trendCostKey(MODEL_OTHER_KEY)]).toBeCloseTo(0.3);
    expect(folded.points[0]?.m7).toBeUndefined();
  });

  it('does not invent Other when there are at most seven models with volume', () => {
    const folded = foldTrendTail(points, ['opus', 'sonnet']);
    expect(folded.keys).toEqual(['opus', 'sonnet']);
    expect(folded.points[0]).toMatchObject({ date: '2026-08-26', opus: 100, sonnet: 40 });
  });

  it('accumulates tokens and cost per series', () => {
    const keys = ['opus', 'sonnet'];
    const cumulative = accumulateTrendSeries(points, keys);
    expect(cumulative[0]).toMatchObject({ opus: 100, sonnet: 40, [trendCostKey('opus')]: 1.5 });
    expect(cumulative[1]).toMatchObject({ opus: 120, sonnet: 120, [trendCostKey('opus')]: 1.75 });
    expect(sumTrendSeriesTokens(cumulative[1], keys)).toBe(240);
    expect(trendSharePct(50, 65)).toBe('76.9%');
    expect(trendSharePct(0, 0)).toBe('0%');
  });
});
