import { afterEach, describe, expect, it, vi } from 'vitest';
import { createTranslator } from '@/lib/i18n';
import {
  backupCardIdentity,
  backupFileLabel,
  backupFileLabels,
  backupNoteSubtitle,
  backupRowTitle,
  fmtAbsoluteI18n,
  fmtRelativeI18n,
  isInternalBackupNote,
} from './backup-format';

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

describe('backupRowTitle', () => {
  it('uses a human kind + time and ignores internal switch notes', () => {
    vi.useFakeTimers({ now: NOW });
    const title = backupRowTitle(
      {
        kind: 'auto-switch',
        createdAt: isoMinutesAgo(120),
      },
      tZh,
    );
    expect(title).toBe('切换前自动 · 2 小时前');
    expect(title).not.toMatch(/before provider|adapter-bridge|452e70db/i);
    expect(
      isInternalBackupNote(
        'before provider switch to claude-grok-adapter-bridge-grok-live-452e70db-ffff',
      ),
    ).toBe(true);
    expect(
      backupNoteSubtitle(
        'before provider switch to claude-grok-adapter-bridge-grok-live-452e70db-ffff',
      ),
    ).toBeNull();
  });

  it('labels blank Grok / manual notes', () => {
    vi.useFakeTimers({ now: NOW });
    expect(backupRowTitle({ kind: 'manual', createdAt: isoMinutesAgo(1) }, tZh)).toBe(
      '手动备份 · 1 分钟前',
    );
    expect(backupRowTitle({ kind: 'manual', createdAt: isoMinutesAgo(1) }, tEn)).toBe(
      'Manual backup · 1 min ago',
    );
    expect(isInternalBackupNote(undefined)).toBe(true);
    expect(isInternalBackupNote('')).toBe(true);
    expect(backupNoteSubtitle(undefined)).toBeNull();
    expect(backupNoteSubtitle('Dashboard 手动备份')).toBe('Dashboard 手动备份');
  });

  it('hides after-provider-upsert notes from the visible title', () => {
    expect(isInternalBackupNote('after provider upsert')).toBe(true);
    expect(isInternalBackupNote('after provider update')).toBe(true);
    expect(backupNoteSubtitle('after provider upsert claude')).toBeNull();
    expect(
      backupRowTitle({ kind: 'auto-switch', createdAt: isoMinutesAgo(3) }, tZh),
    ).not.toMatch(/after provider|upsert/i);
  });
});

describe('backupCardIdentity', () => {
  it('prefers a user note, then extracted identity, then file names', () => {
    expect(backupFileLabel('~/.grok/auth.json')).toBe('auth.json');
    expect(backupFileLabels(['~/.grok/auth.json', '~/.grok/config.toml'])).toBe(
      'auth.json · config.toml',
    );
    expect(
      backupCardIdentity({
        note: 'Dashboard 手动备份',
        identity: 'a@example.com',
        files: ['auth.json'],
      }),
    ).toBe('Dashboard 手动备份');
    expect(
      backupCardIdentity({
        note: 'before provider switch',
        identity: 'a@example.com',
        files: ['auth.json'],
      }),
    ).toBe('a@example.com');
    expect(
      backupCardIdentity({
        files: ['~/.claude/settings.json', '~/.claude.json'],
      }),
    ).toBe('settings.json · .claude.json');
  });

  it('does not treat internal switch notes as the card title', () => {
    expect(
      backupCardIdentity({
        note: 'before provider switch to claude-grok-adapter-bridge',
        files: ['auth.json', 'config.toml'],
      }),
    ).toBe('auth.json · config.toml');
  });
});
