import { describe, expect, it } from 'vitest';

import {
  denseTrendBuckets,
  formatTrendTick,
  formatTrendTooltipLabel,
  localTrendBucket,
  sortUsageTrendTooltipItems,
  trendGrain,
  usageTrendTooltipItemsFromPayload,
  zeroFillTrendSeries,
} from './usage-trend';

describe('trendGrain', () => {
  it('uses hours for a 1-day window and days otherwise', () => {
    expect(trendGrain(1)).toBe('hour');
    expect(trendGrain(0)).toBe('hour');
    expect(trendGrain(7)).toBe('day');
    expect(trendGrain(30)).toBe('day');
  });
});

describe('denseTrendBuckets', () => {
  const now = new Date(2026, 7, 26, 15, 30, 0);

  it('fills local hours for a rolling 24h window', () => {
    const keys = denseTrendBuckets(1, undefined, now);
    expect(keys[0]).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:00$/);
    expect(keys.at(-1)).toBe('2026-08-26 15:00');
    expect(keys.length).toBeGreaterThanOrEqual(24);
    expect(keys.length).toBeLessThanOrEqual(26);
    expect(new Set(keys).size).toBe(keys.length);
  });

  it('fills from local midnight when since clips today', () => {
    const midnight = new Date(2026, 7, 26, 0, 0, 0).toISOString();
    const keys = denseTrendBuckets(1, midnight, now);
    expect(keys[0]).toBe('2026-08-26 00:00');
    expect(keys.at(-1)).toBe('2026-08-26 15:00');
    expect(keys).toHaveLength(16);
  });

  it('fills local calendar days for a 7-day window', () => {
    const keys = denseTrendBuckets(7, undefined, now);
    expect(keys.at(-1)).toBe('2026-08-26');
    expect(keys.every((k) => k.length === 10)).toBe(true);
    expect(keys.length).toBeGreaterThanOrEqual(7);
    expect(keys.length).toBeLessThanOrEqual(9);
  });
});

describe('localTrendBucket', () => {
  it('formats local hour and day from an ISO timestamp', () => {
    const iso = new Date(2026, 7, 26, 9, 15, 0).toISOString();
    expect(localTrendBucket(iso, 'hour')).toBe('2026-08-26 09:00');
    expect(localTrendBucket(iso, 'day')).toBe('2026-08-26');
    expect(localTrendBucket('not-a-date', 'hour')).toBeNull();
  });
});

describe('formatTrendTick / tooltip', () => {
  it('shortens daily and hourly labels', () => {
    expect(formatTrendTick('2026-08-26')).toBe('08-26');
    expect(formatTrendTick('2026-08-26 14:00')).toBe('14:00');
    expect(formatTrendTick('2026-08-26 00:00')).toBe('08-26');
  });

  it('keeps the calendar day on hourly tooltip labels', () => {
    expect(formatTrendTooltipLabel('2026-08-26 14:00')).toBe('08-26 14:00');
    expect(formatTrendTooltipLabel('2026-08-26')).toBe('2026-08-26');
  });
});

describe('zeroFillTrendSeries', () => {
  it('writes 0 for missing agent keys', () => {
    const filled = zeroFillTrendSeries([{ date: '2026-08-26 10:00', claude: 4 }], [
      'claude',
      'codex',
    ]);
    expect(filled).toEqual([{ date: '2026-08-26 10:00', claude: 4, codex: 0 }]);
  });
});

describe('usage trend tooltip items', () => {
  it('drops empty series and sorts by token usage desc', () => {
    const items = sortUsageTrendTooltipItems([
      { key: 'b', name: 'Codex', tokens: 80 },
      { key: 'a', name: 'Claude', tokens: 120 },
      { key: 'c', name: 'Empty', tokens: 0 },
    ]);
    expect(items.map((item) => item.key)).toEqual(['a', 'b']);
  });

  it('maps chart payload names and ignores zero values', () => {
    const items = usageTrendTooltipItemsFromPayload(
      [
        { dataKey: 'codex', name: 'codex', value: 80, color: '#111' },
        { dataKey: 'claude', name: 'claude', value: 240, color: '#222' },
        { dataKey: 'gemini', name: 'gemini', value: 0, color: '#333' },
      ],
      (key) => key.toUpperCase(),
    );
    expect(items).toEqual([
      { key: 'claude', name: 'CLAUDE', tokens: 240, color: '#222' },
      { key: 'codex', name: 'CODEX', tokens: 80, color: '#111' },
    ]);
  });
});
