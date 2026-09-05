import { readStorageItem } from '@/lib/storage-key';

export type NavWidthPolicy = {
  defaultWidth: number;
  collapsedWidth: number;
  minRatio: number;
  maxRatio: number;
  minPx: number;
  minCap: number;
  step: number;
  stepLarge: number;
};

/** Matches the previous expanded primary rail (`w-56`). */
export const PRIMARY_NAV_WIDTH = {
  defaultWidth: 224,
  collapsedWidth: 56,
  minRatio: 0.12,
  maxRatio: 0.28,
  minPx: 176,
  minCap: 200,
  step: 16,
  stepLarge: 48,
} as const satisfies NavWidthPolicy;

/** Matches the previous expanded routes rail (`lg:w-48`); compact is `w-12`. */
export const ROUTES_NAV_WIDTH = {
  defaultWidth: 192,
  collapsedWidth: 48,
  minRatio: 0.1,
  maxRatio: 0.22,
  minPx: 144,
  minCap: 168,
  step: 16,
  stepLarge: 48,
} as const satisfies NavWidthPolicy;

export const SIDEBAR_WIDTH_DEFAULT = PRIMARY_NAV_WIDTH.defaultWidth;
export const SIDEBAR_WIDTH_COLLAPSED = PRIMARY_NAV_WIDTH.collapsedWidth;
export const SIDEBAR_WIDTH_MIN_RATIO = PRIMARY_NAV_WIDTH.minRatio;
export const SIDEBAR_WIDTH_MAX_RATIO = PRIMARY_NAV_WIDTH.maxRatio;
export const SIDEBAR_WIDTH_MIN_PX = PRIMARY_NAV_WIDTH.minPx;
export const SIDEBAR_WIDTH_MIN_CAP = PRIMARY_NAV_WIDTH.minCap;
export const SIDEBAR_WIDTH_STEP = PRIMARY_NAV_WIDTH.step;
export const SIDEBAR_WIDTH_STEP_LARGE = PRIMARY_NAV_WIDTH.stepLarge;

export function navWidthBounds(
  viewportWidth: number,
  policy: NavWidthPolicy,
): { min: number; max: number } {
  if (!(viewportWidth > 0)) {
    return { min: policy.minPx, max: Number.POSITIVE_INFINITY };
  }
  const ratioMin = Math.round(viewportWidth * policy.minRatio);
  const ratioMax = Math.round(viewportWidth * policy.maxRatio);
  const min = Math.min(policy.minCap, Math.max(policy.minPx, ratioMin));
  const max = Math.max(min, ratioMax);
  return { min, max };
}

export function clampNavWidth(
  width: number,
  viewportWidth: number,
  policy: NavWidthPolicy,
): number {
  const requested = Math.round(Number.isFinite(width) ? width : policy.defaultWidth);
  const { min, max } = navWidthBounds(viewportWidth, policy);
  if (!Number.isFinite(max)) return Math.max(min, requested);
  return Math.min(max, Math.max(min, requested));
}

export function sidebarWidthBounds(viewportWidth: number): { min: number; max: number } {
  return navWidthBounds(viewportWidth, PRIMARY_NAV_WIDTH);
}

export function clampSidebarWidth(width: number, viewportWidth: number): number {
  return clampNavWidth(width, viewportWidth, PRIMARY_NAV_WIDTH);
}

export function readStoredSidebarWidth(
  storageKey: string,
  fallback: number = SIDEBAR_WIDTH_DEFAULT,
): number {
  if (typeof window === 'undefined') return fallback;
  try {
    const raw = readStorageItem(window.localStorage, storageKey);
    const n = raw ? Number(raw) : NaN;
    if (Number.isFinite(n) && n > 0) return Math.round(n);
  } catch {
    /* ignore */
  }
  return fallback;
}

export function persistSidebarWidth(storageKey: string, width: number): void {
  try {
    window.localStorage.setItem(storageKey, String(width));
  } catch {
    /* ignore */
  }
}
