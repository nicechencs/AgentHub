import { describe, expect, it } from 'vitest';

import {
  buildUsageSyncStatusLine,
  computeNextCollectAt,
  formatDurationShort,
  formatLastCollectLabel,
  formatNextCollectLabel,
  normalizeIntervalMin,
} from './usage-sync';

describe('normalizeIntervalMin', () => {
  it('treats non-positive as manual-only', () => {
    expect(normalizeIntervalMin(0)).toBe(0);
    expect(normalizeIntervalMin(-1)).toBe(0);
    expect(normalizeIntervalMin(NaN)).toBe(0);
  });

  it('floors and clamps', () => {
    expect(normalizeIntervalMin(30.9)).toBe(30);
    expect(normalizeIntervalMin(99999)).toBe(24 * 60);
  });
});

describe('computeNextCollectAt', () => {
  const now = 1_000_000;

  it('returns null when manual only', () => {
    expect(computeNextCollectAt(null, 0, now)).toBeNull();
  });

  it('schedules from now when never collected', () => {
    expect(computeNextCollectAt(null, 30, now)).toBe(now + 30 * 60_000);
  });

  it('uses last + interval and clamps overdue to now', () => {
    // 2 min ago with 1 min interval → overdue → now
    expect(computeNextCollectAt(now - 120_000, 1, now)).toBe(now);
    // still within window
    expect(computeNextCollectAt(now - 10_000, 30, now)).toBe(now - 10_000 + 30 * 60_000);
  });
});

describe('status labels', () => {
  it('formats short durations', () => {
    expect(formatDurationShort(5_000)).toBe('5 秒');
    expect(formatDurationShort(65_000)).toBe('1 分 5 秒');
    expect(formatDurationShort(3_600_000)).toBe('1 小时');
  });

  it('builds last/next labels', () => {
    const now = 1_000_000;
    expect(formatLastCollectLabel(null, now)).toBe('尚未同步');
    expect(formatLastCollectLabel(now - 5_000, now)).toBe('上次同步：刚刚');
    expect(formatNextCollectLabel(null, 0, now)).toBe('仅手动采集');
    expect(formatNextCollectLabel(now + 90_000, 30, now)).toContain('自动同步');
  });

  it('status line is next-sync only (no last-sync)', () => {
    const now = 1_000_000;
    expect(
      buildUsageSyncStatusLine({
        lastCollectAt: null,
        nextCollectAt: null,
        intervalMin: 0,
        collecting: false,
        now,
      }),
    ).toBe('仅手动采集');

    const countdown = buildUsageSyncStatusLine({
      lastCollectAt: now - 60_000,
      nextCollectAt: now + 90_000,
      intervalMin: 30,
      collecting: false,
      now,
    });
    expect(countdown).toContain('自动同步');
    expect(countdown).not.toContain('上次同步');
    expect(countdown).not.toContain('尚未同步');
  });
});
