import { readLegacy, removeStorageItem, StorageKey } from '@/lib/storage-key';
import { TYPE_SCALE, typeScalePx } from '@/styles/tokens';

export const COMPOSER_LINE_PX = Math.round(
  typeScalePx('body') * Number.parseFloat(TYPE_SCALE.body.lineHeight),
);
export const COMPOSER_SPLIT_MIN_LINES = 2;
export const COMPOSER_TWO_LINE_CONTENT_PX = COMPOSER_LINE_PX * COMPOSER_SPLIT_MIN_LINES;

/** textarea `pt-3 pb-2` */
const TEXTAREA_PAD_Y_PX = 20;
/** toolbar `py-2` + `h-7` + `border-t` */
const TOOLBAR_Y_PX = 16 + 28 + 1;
/** composer shell top+bottom border */
const SHELL_BORDER_Y_PX = 2;

export const COMPOSER_TWO_LINE_FIELD_PX = COMPOSER_TWO_LINE_CONTENT_PX + TEXTAREA_PAD_Y_PX;
export const COMPOSER_PANE_MIN = COMPOSER_TWO_LINE_FIELD_PX + TOOLBAR_Y_PX + SHELL_BORDER_Y_PX;
export const COMPOSER_MAX_SHARE = 0.5;
export const COMPOSER_PANE_STEP = 16;
export const COMPOSER_PANE_STEP_LARGE = 48;

export const COMPOSER_PANE_HEIGHT_STORAGE_KEY = StorageKey.chatComposerPaneHeight;

export function readStoredComposerPaneHeight(): number | null {
  if (typeof window === 'undefined') return null;
  try {
    const raw = readLegacy(window.localStorage, COMPOSER_PANE_HEIGHT_STORAGE_KEY);
    if (raw == null || raw === '') return null;
    const n = Number(raw);
    if (Number.isFinite(n) && n > 0) return Math.round(n);
  } catch {
    /* ignore */
  }
  return null;
}

export function persistComposerPaneHeight(height: number | null): void {
  try {
    if (height == null) {
      removeStorageItem(window.localStorage, COMPOSER_PANE_HEIGHT_STORAGE_KEY);
      return;
    }
    window.localStorage.setItem(COMPOSER_PANE_HEIGHT_STORAGE_KEY, String(height));
  } catch {
    /* ignore */
  }
}

export function composerPaneMaxHeight(stageHeight: number): number {
  return Math.floor(Math.max(0, stageHeight) * COMPOSER_MAX_SHARE);
}

/** Composer pane: ≥ two line-heights (plus chrome), ≤ half the stage. */
export function clampComposerPaneHeight(height: number, stageHeight: number): number {
  const requested = Math.round(Number.isFinite(height) ? height : COMPOSER_PANE_MIN);
  if (stageHeight <= 0) {
    return Math.max(COMPOSER_PANE_MIN, requested);
  }
  const maxH = composerPaneMaxHeight(stageHeight);
  const minH = Math.min(COMPOSER_PANE_MIN, maxH);
  return Math.min(maxH, Math.max(minH, requested));
}
