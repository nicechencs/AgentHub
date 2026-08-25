import { pageEdgePx } from '@/components/layout/page-rhythm';

export const SIDE_SPLIT_WIDTH_DEFAULT = 440;
/** Comfortable drag / remembered minimum. */
export const SIDE_SPLIT_WIDTH_MIN = 300;
/** Hard floor on a very narrow workbench. */
export const SIDE_SPLIT_WIDTH_FLOOR = 240;
/** List column reserved while the inspect pane is open. */
export const SIDE_SPLIT_MAIN_MIN = 380;
export const SIDE_SPLIT_MAIN_FLOOR = 280;
export const SIDE_SPLIT_FRAME_PAD_RIGHT = pageEdgePx.x;
export const SIDE_SPLIT_FRAME_PAD_Y = pageEdgePx.previewY;
export const SIDE_SPLIT_SEPARATOR_W = pageEdgePx.separator;
export const SIDE_SPLIT_WIDTH_STEP = 16;
export const SIDE_SPLIT_WIDTH_STEP_LARGE = 48;

export function readStoredSideSplitWidth(
  storageKey: string,
  fallback: number = SIDE_SPLIT_WIDTH_DEFAULT,
): number {
  if (typeof window === 'undefined') return fallback;
  try {
    const raw = window.localStorage.getItem(storageKey);
    const n = raw ? Number(raw) : NaN;
    if (Number.isFinite(n) && n >= SIDE_SPLIT_WIDTH_MIN) return Math.round(n);
  } catch {
    /* ignore */
  }
  return fallback;
}

export function persistSideSplitWidth(storageKey: string, width: number): void {
  try {
    window.localStorage.setItem(storageKey, String(width));
  } catch {
    /* ignore */
  }
}

/** Clamp the inspect card so the list column stays usable. */
export function clampSideSplitWidth(width: number, containerWidth: number): number {
  const chrome = SIDE_SPLIT_SEPARATOR_W + SIDE_SPLIT_FRAME_PAD_RIGHT;
  const usable = Math.max(0, containerWidth - chrome);
  const mainReserve =
    usable >= SIDE_SPLIT_MAIN_MIN + SIDE_SPLIT_WIDTH_MIN
      ? SIDE_SPLIT_MAIN_MIN
      : Math.min(SIDE_SPLIT_MAIN_MIN, Math.max(SIDE_SPLIT_MAIN_FLOOR, Math.floor(usable * 0.48)));
  const maxW = Math.max(SIDE_SPLIT_WIDTH_FLOOR, usable - mainReserve);
  const minW = Math.min(SIDE_SPLIT_WIDTH_MIN, maxW);
  return Math.min(maxW, Math.max(minW, Math.round(width)));
}

export function createIdempotentCleanup<T extends unknown[]>(cleanup: (...args: T) => void) {
  let completed = false;
  return (...args: T) => {
    if (completed) return;
    completed = true;
    cleanup(...args);
  };
}
