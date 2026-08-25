import { describe, expect, it } from 'vitest';
import {
  clampSideSplitWidth,
  SIDE_SPLIT_MAIN_MIN,
  SIDE_SPLIT_WIDTH_DEFAULT,
  SIDE_SPLIT_WIDTH_FLOOR,
  SIDE_SPLIT_WIDTH_MIN,
} from './side-split-model';

describe('clampSideSplitWidth', () => {
  it('keeps a default-sized pane when the workbench is wide', () => {
    expect(clampSideSplitWidth(SIDE_SPLIT_WIDTH_DEFAULT, 1200)).toBe(SIDE_SPLIT_WIDTH_DEFAULT);
  });

  it('does not shrink below the floor even in a narrow pane', () => {
    expect(clampSideSplitWidth(120, 400)).toBeGreaterThanOrEqual(SIDE_SPLIT_WIDTH_FLOOR);
  });

  it('reserves a list column on a medium workbench', () => {
    const width = clampSideSplitWidth(800, 900);
    expect(width).toBeLessThanOrEqual(900 - SIDE_SPLIT_MAIN_MIN);
    expect(width).toBeGreaterThanOrEqual(SIDE_SPLIT_WIDTH_FLOOR);
  });

  it('caps a huge requested width to the remaining space', () => {
    const width = clampSideSplitWidth(4000, 1000);
    expect(width).toBeLessThanOrEqual(SIDE_SPLIT_WIDTH_MIN + 400);
    expect(width).toBeLessThan(1000);
  });
});
