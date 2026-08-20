import { pageEdgePx } from '@/components/layout/page-rhythm';

export const PREVIEW_WIDTH_DEFAULT = 440;
export const PREVIEW_WIDTH_MIN = 300;
export const PREVIEW_WIDTH_FLOOR = 240;
export const MAIN_WIDTH_MIN = 380;
export const MAIN_WIDTH_FLOOR = 280;
export const PREVIEW_FRAME_PAD_RIGHT = pageEdgePx.x;
export const PREVIEW_FRAME_PAD_Y = pageEdgePx.previewY;
export const PREVIEW_SEPARATOR_W = pageEdgePx.separator;
export const PREVIEW_WIDTH_STORAGE_KEY = 'agenthub.projects.previewWidth';
export const PREVIEW_WIDTH_STEP = 16;
export const PREVIEW_WIDTH_STEP_LARGE = 48;

export function readStoredProjectPreviewWidth(): number {
  if (typeof window === 'undefined') return PREVIEW_WIDTH_DEFAULT;
  try {
    const raw = window.localStorage.getItem(PREVIEW_WIDTH_STORAGE_KEY);
    const n = raw ? Number(raw) : NaN;
    if (Number.isFinite(n) && n >= PREVIEW_WIDTH_MIN) return Math.round(n);
  } catch {
    /* ignore */
  }
  return PREVIEW_WIDTH_DEFAULT;
}

export function persistProjectPreviewWidth(width: number): void {
  try {
    window.localStorage.setItem(PREVIEW_WIDTH_STORAGE_KEY, String(width));
  } catch {
    /* ignore */
  }
}

/** Clamp preview card width so the list keeps a usable column. */
export function clampProjectPreviewWidth(width: number, containerWidth: number): number {
  const chrome = PREVIEW_SEPARATOR_W + PREVIEW_FRAME_PAD_RIGHT;
  const usable = Math.max(0, containerWidth - chrome);
  const mainReserve =
    usable >= MAIN_WIDTH_MIN + PREVIEW_WIDTH_MIN
      ? MAIN_WIDTH_MIN
      : Math.min(MAIN_WIDTH_MIN, Math.max(MAIN_WIDTH_FLOOR, Math.floor(usable * 0.48)));
  const maxW = Math.max(PREVIEW_WIDTH_FLOOR, usable - mainReserve);
  const minW = Math.min(PREVIEW_WIDTH_MIN, maxW);
  return Math.min(maxW, Math.max(minW, Math.round(width)));
}
