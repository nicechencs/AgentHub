import { afterEach, describe, expect, it, vi } from 'vitest';
import { StorageKey } from '@/lib/storage-key';
import {
  clampNavWidth,
  clampSidebarWidth,
  persistSidebarWidth,
  readStoredSidebarWidth,
  ROUTES_NAV_WIDTH,
  SIDEBAR_WIDTH_DEFAULT,
  SIDEBAR_WIDTH_MAX_RATIO,
  SIDEBAR_WIDTH_MIN_CAP,
  SIDEBAR_WIDTH_MIN_PX,
  SIDEBAR_WIDTH_MIN_RATIO,
  sidebarWidthBounds,
} from './sidebar-width-model';

describe('sidebarWidthBounds', () => {
  it('uses the pixel floor when 12% of a narrow window would crush labels', () => {
    const { min, max } = sidebarWidthBounds(1000);
    expect(min).toBe(SIDEBAR_WIDTH_MIN_PX);
    expect(max).toBe(Math.round(1000 * SIDEBAR_WIDTH_MAX_RATIO));
    expect(min).toBeGreaterThan(1000 * 0.08);
    expect(min).toBeLessThan(1000 * SIDEBAR_WIDTH_MIN_RATIO + 80);
  });

  it('caps the ratio floor so a large window still leaves room below the default', () => {
    const { min, max } = sidebarWidthBounds(1920);
    expect(min).toBe(SIDEBAR_WIDTH_MIN_CAP);
    expect(min).toBeLessThan(SIDEBAR_WIDTH_DEFAULT);
    expect(max).toBe(Math.round(1920 * SIDEBAR_WIDTH_MAX_RATIO));
    expect(max).toBeGreaterThan(SIDEBAR_WIDTH_DEFAULT);
  });

  it('keeps min at or below max', () => {
    for (const vw of [640, 800, 1024, 1280, 1440, 1920, 2560]) {
      const { min, max } = sidebarWidthBounds(vw);
      expect(min).toBeLessThanOrEqual(max);
      expect(min).toBeGreaterThanOrEqual(SIDEBAR_WIDTH_MIN_PX);
      expect(min).toBeLessThanOrEqual(SIDEBAR_WIDTH_MIN_CAP);
    }
  });
});

describe('clampSidebarWidth', () => {
  it('keeps the default width on a typical desktop window', () => {
    expect(clampSidebarWidth(SIDEBAR_WIDTH_DEFAULT, 1280)).toBe(SIDEBAR_WIDTH_DEFAULT);
    expect(clampSidebarWidth(SIDEBAR_WIDTH_DEFAULT, 1920)).toBe(SIDEBAR_WIDTH_DEFAULT);
  });

  it('clamps a drag to the window share', () => {
    expect(clampSidebarWidth(80, 1280)).toBe(sidebarWidthBounds(1280).min);
    expect(clampSidebarWidth(900, 1280)).toBe(sidebarWidthBounds(1280).max);
  });

  it('does not shrink a stored width before the window is measured', () => {
    expect(clampSidebarWidth(300, 0)).toBe(300);
  });
});

describe('ROUTES_NAV_WIDTH', () => {
  it('keeps the default routes rail on a typical desktop window', () => {
    expect(clampNavWidth(ROUTES_NAV_WIDTH.defaultWidth, 1280, ROUTES_NAV_WIDTH)).toBe(
      ROUTES_NAV_WIDTH.defaultWidth,
    );
    expect(clampNavWidth(ROUTES_NAV_WIDTH.defaultWidth, 1920, ROUTES_NAV_WIDTH)).toBe(
      ROUTES_NAV_WIDTH.defaultWidth,
    );
  });

  it('uses a tighter share than the primary rail so two navs can sit together', () => {
    expect(ROUTES_NAV_WIDTH.maxRatio).toBeLessThan(0.28);
    expect(ROUTES_NAV_WIDTH.minRatio).toBeLessThan(0.12);
    expect(clampNavWidth(80, 1280, ROUTES_NAV_WIDTH)).toBeGreaterThanOrEqual(ROUTES_NAV_WIDTH.minPx);
    expect(clampNavWidth(900, 1280, ROUTES_NAV_WIDTH)).toBe(
      Math.round(1280 * ROUTES_NAV_WIDTH.maxRatio),
    );
  });
});

describe('sidebar width persistence', () => {
  const store = new Map<string, string>();
  const localStorage = {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => {
      store.set(key, value);
    },
    removeItem: (key: string) => {
      store.delete(key);
    },
  };

  afterEach(() => {
    store.clear();
    vi.unstubAllGlobals();
  });

  it('reads a remembered pixel width and falls back to the default', () => {
    vi.stubGlobal('window', { localStorage });
    expect(readStoredSidebarWidth(StorageKey.sidebarWidth)).toBe(SIDEBAR_WIDTH_DEFAULT);
    persistSidebarWidth(StorageKey.sidebarWidth, 280);
    expect(readStoredSidebarWidth(StorageKey.sidebarWidth)).toBe(280);
  });
});
