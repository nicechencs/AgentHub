import { describe, expect, it } from 'vitest';
import type { LogLevel } from '@/lib/types';
import { guiErrorCode } from './settings';
import {
  closeToTraySettingValue,
  resolveCloseToTray,
  resolveUsageCollectIntervalMin,
} from '@/lib/backend/tauri/settings';

/** Mirror parse rules used by settings API (keep in sync with settings.ts). */
const LOG_LEVELS: LogLevel[] = ['error', 'warn', 'info', 'debug', 'trace'];

function parseLogLevel(raw: string | undefined | null): LogLevel {
  const v = (raw ?? 'info').trim().toLowerCase();
  return (LOG_LEVELS as string[]).includes(v) ? (v as LogLevel) : 'info';
}

function mapLanguageToUi(raw: string): 'zh' | 'en' {
  const l = raw.toLowerCase();
  if (l.startsWith('en')) return 'en';
  return 'zh';
}

function mapLanguageToCore(ui: 'zh' | 'en'): string {
  return ui === 'en' ? 'en' : 'zh-CN';
}

describe('settings log helpers', () => {
  it('parseLogLevel accepts canonical levels and falls back', () => {
    expect(parseLogLevel('DEBUG')).toBe('debug');
    expect(parseLogLevel('warn')).toBe('warn');
    expect(parseLogLevel('nope')).toBe('info');
    expect(parseLogLevel(null)).toBe('info');
  });

  it('language maps between UI and core', () => {
    expect(mapLanguageToUi('zh-CN')).toBe('zh');
    expect(mapLanguageToUi('en-US')).toBe('en');
    expect(mapLanguageToCore('zh')).toBe('zh-CN');
    expect(mapLanguageToCore('en')).toBe('en');
  });

  it('retention days clamp rules used by UI', () => {
    const clamp = (n: number) => Math.min(365, Math.max(1, n));
    expect(clamp(0)).toBe(1);
    expect(clamp(14)).toBe(14);
    expect(clamp(400)).toBe(365);
  });

  it('prefers a structured backend code and accepts retryable adapter codes', () => {
    expect(guiErrorCode({
      code: 'adapter.bridge_start',
      message: '本机转发无法启动或停止，请点重试。',
    })).toBe('adapter.bridge_start');
    expect(guiErrorCode({
      code: 'retryable:adapter.port_in_use',
      message: '端口被占用',
    })).toBe('retryable:adapter.port_in_use');
  });

  it('falls back to bracketed codes but rejects arbitrary secret-like values', () => {
    expect(guiErrorCode(new Error('listener failed [adapter.bridge_start]')))
      .toBe('adapter.bridge_start');
    expect(guiErrorCode({ code: 'sk-live-example-secret', message: 'listener failed' }))
      .toBeUndefined();
    expect(guiErrorCode({ code: 'sk.live.secret.value', message: 'listener failed' }))
      .toBeUndefined();
    expect(guiErrorCode({ code: 'eyJhbGciOiJIUzI1NiJ9.e30.sig', message: 'listener failed' }))
      .toBeUndefined();
    expect(guiErrorCode({ code: 'raw key value', message: 'listener failed [adapter.bridge_stop]' }))
      .toBe('adapter.bridge_stop');
  });
});

describe('closeToTray helpers (shared with tauri settings port)', () => {
  it('resolves core over local', () => {
    expect(resolveCloseToTray(false, true)).toBe(false);
    expect(resolveCloseToTray(undefined, false)).toBe(false);
    expect(closeToTraySettingValue(false)).toBe('false');
  });
});

describe('usageCollectIntervalMin helpers (shared with tauri settings port)', () => {
  it('resolves core over local, including 0', () => {
    expect(resolveUsageCollectIntervalMin(45, 15)).toBe(45);
    expect(resolveUsageCollectIntervalMin(0, 30)).toBe(0);
    expect(resolveUsageCollectIntervalMin(undefined, 15)).toBe(15);
    expect(resolveUsageCollectIntervalMin(undefined, undefined)).toBe(30);
  });
});
