import { afterEach, describe, expect, it, vi } from 'vitest';

const isTauriMock = vi.fn();

vi.mock('@tauri-apps/api/core', () => ({
  isTauri: () => isTauriMock(),
  invoke: vi.fn(),
}));

import {
  FEATURE_NOT_WIRED,
  detectHostPlatform,
  getRuntimeInstallChannel,
  isTauriApp,
  notWiredError,
} from '@/lib/platform';

describe('isTauriApp', () => {
  afterEach(() => {
    isTauriMock.mockReset();
  });

  it('returns true when Tauri runtime flag is set', () => {
    isTauriMock.mockReturnValue(true);
    expect(isTauriApp()).toBe(true);
  });

  it('returns false in browser / Vite prototype', () => {
    isTauriMock.mockReturnValue(false);
    expect(isTauriApp()).toBe(false);
  });

  it('returns false if isTauri throws', () => {
    isTauriMock.mockImplementation(() => {
      throw new Error('no globalThis');
    });
    expect(isTauriApp()).toBe(false);
  });
});

describe('notWiredError', () => {
  it('uses shared 功能尚未接入 copy', () => {
    expect(notWiredError().message).toBe(FEATURE_NOT_WIRED);
    expect(notWiredError('Agent 安装').message).toBe(`Agent 安装：${FEATURE_NOT_WIRED}`);
  });
});

describe('runtime install platform helpers', () => {
  it('recognises macOS from navigator-style values and selects Homebrew', () => {
    expect(
      detectHostPlatform({ platform: 'MacIntel', userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 14_5)' }),
    ).toBe('macos');
    expect(getRuntimeInstallChannel('macos')).toBe('brew');
  });

  it('keeps winget for Windows and Linux/unknown fallback hosts', () => {
    expect(detectHostPlatform({ platform: 'Win32', userAgent: '' })).toBe('windows');
    expect(getRuntimeInstallChannel('windows')).toBe('winget');
    expect(getRuntimeInstallChannel('linux')).toBe('winget');
    expect(getRuntimeInstallChannel('unknown')).toBe('winget');
  });
});
