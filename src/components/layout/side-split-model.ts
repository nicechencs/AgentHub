import { pageEdgePx } from '@/components/layout/page-rhythm';
import { readStorageItem } from '@/lib/storage-key';

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
/**
 * Inspect pane share of the workbench. List reserve (`SIDE_SPLIT_MAIN_MIN`)
 * still wins when it is tighter; this only stops a stored width from eating
 * the list after the window shrinks.
 */
export const SIDE_SPLIT_MAX_SHARE = 0.7;

export function readStoredSideSplitWidth(
  storageKey: string,
  fallback: number = SIDE_SPLIT_WIDTH_DEFAULT,
): number {
  if (typeof window === 'undefined') return fallback;
  try {
    const raw = readStorageItem(window.localStorage, storageKey);
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
  const requested = Math.round(Number.isFinite(width) ? width : SIDE_SPLIT_WIDTH_DEFAULT);
  if (containerWidth <= 0) {
    return Math.max(SIDE_SPLIT_WIDTH_FLOOR, requested);
  }
  const chrome = SIDE_SPLIT_SEPARATOR_W + SIDE_SPLIT_FRAME_PAD_RIGHT;
  const usable = Math.max(0, containerWidth - chrome);
  const mainReserve =
    usable >= SIDE_SPLIT_MAIN_MIN + SIDE_SPLIT_WIDTH_MIN
      ? SIDE_SPLIT_MAIN_MIN
      : Math.min(SIDE_SPLIT_MAIN_MIN, Math.max(SIDE_SPLIT_MAIN_FLOOR, Math.floor(usable * 0.48)));
  const listCap = Math.max(0, usable - mainReserve);
  const shareCap = Math.floor(usable * SIDE_SPLIT_MAX_SHARE);
  const maxW = Math.max(0, Math.min(listCap, shareCap));
  const minW = Math.min(SIDE_SPLIT_WIDTH_MIN, maxW);
  return Math.min(maxW, Math.max(minW, requested));
}

export function createIdempotentCleanup<T extends unknown[]>(cleanup: (...args: T) => void) {
  let completed = false;
  return (...args: T) => {
    if (completed) return;
    completed = true;
    cleanup(...args);
  };
}
