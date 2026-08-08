import { afterEach, describe, expect, it, vi } from 'vitest';

const isTauriMock = vi.fn();

vi.mock('@tauri-apps/api/core', () => ({
  isTauri: () => isTauriMock(),
  invoke: vi.fn(),
}));

import { FEATURE_NOT_WIRED, isTauriApp, notWiredError } from '@/lib/platform';

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
