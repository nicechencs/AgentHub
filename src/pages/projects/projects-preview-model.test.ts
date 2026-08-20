import { describe, expect, it } from 'vitest';
import {
  clampProjectPreviewWidth,
  MAIN_WIDTH_MIN,
  PREVIEW_WIDTH_DEFAULT,
  PREVIEW_WIDTH_FLOOR,
  PREVIEW_WIDTH_MIN,
} from './projects-preview-model';

describe('clampProjectPreviewWidth', () => {
  it('keeps a default-sized pane when the workbench is wide', () => {
    expect(clampProjectPreviewWidth(PREVIEW_WIDTH_DEFAULT, 1200)).toBe(PREVIEW_WIDTH_DEFAULT);
  });

  it('does not shrink below the floor even in a narrow pane', () => {
    expect(clampProjectPreviewWidth(120, 400)).toBeGreaterThanOrEqual(PREVIEW_WIDTH_FLOOR);
  });

  it('reserves a list column on a medium workbench', () => {
    const width = clampProjectPreviewWidth(800, 900);
    expect(width).toBeLessThanOrEqual(900 - MAIN_WIDTH_MIN);
    expect(width).toBeGreaterThanOrEqual(PREVIEW_WIDTH_FLOOR);
  });

  it('caps a huge requested width to the remaining space', () => {
    const width = clampProjectPreviewWidth(4000, 1000);
    expect(width).toBeLessThanOrEqual(PREVIEW_WIDTH_MIN + 400);
    expect(width).toBeLessThan(1000);
  });
});
