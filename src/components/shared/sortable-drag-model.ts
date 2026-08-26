/** Row marker used by pointer hit-testing. HTML5 DnD is not used: Tauri intercepts it. */
export const SORTABLE_ID_ATTR = 'data-sortable-id';
export const SORTABLE_PREVIEW_ATTR = 'data-sortable-preview';
export const SORTABLE_PREVIEW_SCALE = 1.02;
export const SORTABLE_PREVIEW_Z = '70';

export function grabOffset(
  pointerX: number,
  pointerY: number,
  rowLeft: number,
  rowTop: number,
): { x: number; y: number } {
  return { x: pointerX - rowLeft, y: pointerY - rowTop };
}

export function previewOrigin(
  pointerX: number,
  pointerY: number,
  offsetX: number,
  offsetY: number,
): { x: number; y: number } {
  return { x: pointerX - offsetX, y: pointerY - offsetY };
}

export function previewTransform(x: number, y: number, scale = SORTABLE_PREVIEW_SCALE): string {
  return `translate3d(${x}px, ${y}px, 0) scale(${scale})`;
}
