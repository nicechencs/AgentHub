import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  displayTitle,
  extraPreview,
  fmtBytes,
  nativeSessionId,
  projectDisplayPath,
  titleHoverLabel,
  relativeTime,
  shortPath,
  shortSessionId,
} from './project-format';

const NOW = Date.parse('2026-08-18T12:00:00.000Z');

afterEach(() => {
  vi.useRealTimers();
});

describe('displayTitle', () => {
  it('prefers a trimmed alias over the native title', () => {
    expect(displayTitle({ title: 'app', alias: '  工作区  ' })).toBe('工作区');
    expect(displayTitle({ title: 'app', alias: '   ' })).toBe('app');
    expect(displayTitle({ title: 'app' })).toBe('app');
  });
});

describe('projectDisplayPath', () => {
  it('prefers the workspace path, then relative, then storage', () => {
    expect(
      projectDisplayPath({
        actualPath: 'C:\\Users\\demo\\app',
        relativePath: 'projects/-C-Users-demo-app',
        storagePath: 'C:\\Users\\demo\\.claude\\projects\\-C-Users-demo-app',
      }),
    ).toBe('C:\\Users\\demo\\app');
    expect(
      projectDisplayPath({
        actualPath: '  ',
        relativePath: 'sessions',
        storagePath: 'C:\\Users\\demo\\.kimi-code\\sessions',
      }),
    ).toBe('sessions');
    expect(
      projectDisplayPath({
        actualPath: null,
        relativePath: '',
        storagePath: 'C:\\Users\\demo\\.grok\\sessions',
      }),
    ).toBe('C:\\Users\\demo\\.grok\\sessions');
  });
});

describe('extraPreview', () => {
  it('hides preview that is the title or a shorter prefix of it', () => {
    expect(extraPreview('修复登录页 token 过期问题', '修复登录页 token 过期问题')).toBeNull();
    expect(extraPreview('修复登录页 token 过期问题…', '修复登录页 token 过期问题')).toBeNull();
    expect(extraPreview('app', 'app')).toBeNull();
    expect(extraPreview('app', '  ')).toBeNull();
  });

  it('keeps only the remainder when preview continues past the title', () => {
    expect(
      extraPreview(
        '修复登录页 token 过期问题',
        '修复登录页 token 过期问题，需要检查 refresh 流程…',
      ),
    ).toBe('需要检查 refresh 流程');
  });

  it('keeps preview when it is a different topic than the folder title', () => {
    expect(extraPreview('app', '修复登录页 token 过期问题，需要检查 refresh 流程…')).toBe(
      '修复登录页 token 过期问题，需要检查 refresh 流程…',
    );
  });
});

describe('titleHoverLabel', () => {
  it('always keeps the title so a truncated row stays readable', () => {
    expect(titleHoverLabel('修复登录页 token 过期问题')).toBe('修复登录页 token 过期问题');
    expect(titleHoverLabel('修复登录页 token 过期问题', '修复登录页 token 过期问题')).toBe(
      '修复登录页 token 过期问题',
    );
    expect(
      titleHoverLabel(
        '修复登录页 token 过期问题',
        '修复登录页 token 过期问题，需要检查 refresh 流程…',
      ),
    ).toBe('修复登录页 token 过期问题\n需要检查 refresh 流程');
  });
});

describe('fmtBytes', () => {
  it('formats bytes / KB / MB', () => {
    expect(fmtBytes(512)).toBe('512 B');
    expect(fmtBytes(1536)).toBe('1.5 KB');
    expect(fmtBytes(192_000)).toBe('187.5 KB');
    expect(fmtBytes(2 * 1024 * 1024)).toBe('2.0 MB');
  });
});

describe('shortPath / shortSessionId', () => {
  it('keeps short values and ellipsizes the head of long ones', () => {
    expect(shortPath('C:\\app', 48)).toBe('C:\\app');
    expect(shortPath('C:\\Users\\demo\\very\\long\\workspace\\path', 16)).toBe(
      '…\\workspace\\path',
    );
    expect(shortSessionId('sess-a1')).toBe('sess-a1');
    expect(shortSessionId('abcdefghijklmnopqrstuvwxyz0123456789', 10)).toBe('abcdefghi…');
  });
});

describe('nativeSessionId', () => {
  it('returns a trimmed id or null', () => {
    expect(nativeSessionId({ sessionId: '  abc  ' })).toBe('abc');
    expect(nativeSessionId({ sessionId: '   ' })).toBeNull();
    expect(nativeSessionId({})).toBeNull();
  });
});

describe('relativeTime', () => {
  it('buckets just now / minutes / hours / days', () => {
    vi.useFakeTimers({ now: NOW });
    expect(relativeTime(new Date(NOW - 30_000).toISOString())).toBe('刚刚');
    expect(relativeTime(new Date(NOW - 5 * 60_000).toISOString())).toBe('5 分钟前');
    expect(relativeTime(new Date(NOW - 3 * 3600_000).toISOString())).toBe('3 小时前');
    expect(relativeTime(new Date(NOW - 2 * 86400_000).toISOString())).toBe('2 天前');
  });

  it('returns empty string for unparseable input', () => {
    expect(relativeTime('not-a-date')).toBe('');
  });
});
