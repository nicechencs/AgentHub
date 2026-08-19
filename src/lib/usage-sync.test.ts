import { describe, expect, it, vi } from 'vitest';

import { createTranslator } from '@/lib/i18n';
import {
  buildUsageSyncStatusLine,
  computeAutoRetryAt,
  computeAutoRetryDelay,
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

describe('automatic retry backoff', () => {
  it('backs off transient failures instead of using the overdue grace delay', () => {
    const now = 1_000_000;
    expect(computeAutoRetryDelay(1, 30)).toBe(30_000);
    expect(computeAutoRetryDelay(2, 30)).toBe(60_000);
    expect(computeAutoRetryAt(now, 30, 1, now)).toBe(now + 30_000);
  });

  it('caps retries at the configured normal interval', () => {
    const now = 1_000_000;
    expect(computeAutoRetryDelay(5, 1)).toBe(60_000);
    expect(computeAutoRetryAt(now, 1, 5, now)).toBe(now + 60_000);
  });

  it('does not schedule an automatic retry in manual-only mode', () => {
    expect(computeAutoRetryAt(1_000, 0, 1, 1_000)).toBeNull();
  });

  it('keeps a failed auto attempt off the 2s overdue timer', () => {
    vi.useFakeTimers({ now: 10_000 });
    try {
      let calls = 0;
      const retryAt = computeAutoRetryAt(Date.now(), 30, 1);
      expect(retryAt).toBe(40_000);
      setTimeout(() => {
        calls += 1;
      }, retryAt! - Date.now());

      vi.advanceTimersByTime(2_000);
      expect(calls).toBe(0);
      vi.advanceTimersByTime(27_999);
      expect(calls).toBe(0);
      vi.advanceTimersByTime(1);
      expect(calls).toBe(1);
    } finally {
      vi.useRealTimers();
    }
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

describe('status labels with translator', () => {
  it('uses en copy when t is passed', () => {
    const t = createTranslator('en');
    const now = 1_000_000;
    expect(formatDurationShort(5_000, t)).toBe('5s');
    expect(formatDurationShort(65_000, t)).toBe('1m 5s');
    expect(formatLastCollectLabel(null, now, t)).toBe('Not synced yet');
    expect(formatNextCollectLabel(null, 0, now, t)).toBe('Manual collect only');
    expect(
      buildUsageSyncStatusLine({
        lastCollectAt: null,
        nextCollectAt: null,
        intervalMin: 0,
        collecting: true,
        now,
        t,
      }),
    ).toBe('Syncing usage…');
    expect(
      buildUsageSyncStatusLine({
        lastCollectAt: null,
        nextCollectAt: null,
        intervalMin: 0,
        collecting: false,
        now,
        t,
      }),
    ).toBe('Manual collect only');
  });
});
