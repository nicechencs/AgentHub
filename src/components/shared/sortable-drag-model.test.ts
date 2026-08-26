import { describe, expect, it } from 'vitest';
import {
  grabOffset,
  previewOrigin,
  previewTransform,
  SORTABLE_PREVIEW_SCALE,
} from './sortable-drag-model';

describe('sortable drag preview geometry', () => {
  it('keeps the grab point glued to the pointer', () => {
    const offset = grabOffset(120, 80, 100, 40);
    expect(offset).toEqual({ x: 20, y: 40 });
    expect(previewOrigin(160, 110, offset.x, offset.y)).toEqual({ x: 140, y: 70 });
  });

  it('moves with translate3d so the clone can follow the pointer', () => {
    expect(previewTransform(140, 70)).toBe(
      `translate3d(140px, 70px, 0) scale(${SORTABLE_PREVIEW_SCALE})`,
    );
    expect(previewTransform(0, 0, 1)).toBe('translate3d(0px, 0px, 0) scale(1)');
  });
});
