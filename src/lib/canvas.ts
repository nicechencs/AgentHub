import {
  DEFAULT_CANVAS_ID,
  isCanvasId,
  type CanvasId,
} from '@/styles/tokens';
import { loadString, saveString, StorageKey } from '@/lib/ui-preferences';

export type { CanvasId };

export function loadStoredCanvas(): CanvasId {
  const value = loadString(StorageKey.canvas, DEFAULT_CANVAS_ID);
  return isCanvasId(value) ? value : DEFAULT_CANVAS_ID;
}

/** Write the selected page background onto `<html data-canvas>`. */
export function applyCanvas(id: CanvasId): void {
  if (typeof document === 'undefined') return;
  document.documentElement.dataset.canvas = id;
}

export function persistCanvas(id: CanvasId): void {
  saveString(StorageKey.canvas, id);
  applyCanvas(id);
}
