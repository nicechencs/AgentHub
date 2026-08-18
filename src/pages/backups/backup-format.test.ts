import { afterEach, describe, expect, it, vi } from 'vitest';
import { createTranslator } from '@/lib/i18n';
import { fmtAbsoluteI18n, fmtRelativeI18n } from './backup-format';

const tZh = createTranslator('zh');
const tEn = createTranslator('en');
const NOW = Date.parse('2026-08-18T12:00:00.000Z');

function isoMinutesAgo(minutes: number): string {
  return new Date(NOW - minutes * 60_000).toISOString();
}

afterEach(() => {
  vi.useRealTimers();
});

describe('fmtRelativeI18n', () => {
  it('returns an em dash when iso is missing', () => {
    expect(fmtRelativeI18n(undefined, tZh)).toBe('—');
    expect(fmtRelativeI18n('', tEn)).toBe('—');
  });

  it('buckets just now / minutes / hours / days', () => {
    vi.useFakeTimers({ now: NOW });

    expect(fmtRelativeI18n(isoMinutesAgo(0), tZh)).toBe('刚刚');
    expect(fmtRelativeI18n(new Date(NOW - 30_000).toISOString(), tEn)).toBe('Just now');

    expect(fmtRelativeI18n(isoMinutesAgo(1), tZh)).toBe('1 分钟前');
    expect(fmtRelativeI18n(isoMinutesAgo(59), tEn)).toBe('59 min ago');

    expect(fmtRelativeI18n(isoMinutesAgo(60), tZh)).toBe('1 小时前');
    expect(fmtRelativeI18n(isoMinutesAgo(23 * 60), tEn)).toBe('23 h ago');

    expect(fmtRelativeI18n(isoMinutesAgo(24 * 60), tZh)).toBe('1 天前');
    expect(fmtRelativeI18n(isoMinutesAgo(48 * 60), tEn)).toBe('2 d ago');
  });
});

describe('fmtAbsoluteI18n', () => {
  it('formats en vs zh with a 24-hour clock', () => {
    const iso = '2026-03-15T14:30:00.000Z';
    const en = fmtAbsoluteI18n(iso, 'en');
    const zh = fmtAbsoluteI18n(iso, 'zh');
    const date = new Date(iso);

    expect(en).toBe(date.toLocaleString('en-US', { hour12: false }));
    expect(zh).toBe(date.toLocaleString('zh-CN', { hour12: false }));
    expect(en).not.toMatch(/AM|PM/i);
    expect(zh).not.toMatch(/AM|PM/i);
    expect(en).not.toBe(zh);
  });
});
