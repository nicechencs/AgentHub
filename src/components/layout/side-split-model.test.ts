import { describe, expect, it } from 'vitest';
import {
  clampSideSplitWidth,
  SIDE_SPLIT_FRAME_PAD_RIGHT,
  SIDE_SPLIT_MAIN_MIN,
  SIDE_SPLIT_MAX_SHARE,
  SIDE_SPLIT_SEPARATOR_W,
  SIDE_SPLIT_WIDTH_DEFAULT,
  SIDE_SPLIT_WIDTH_FLOOR,
  SIDE_SPLIT_WIDTH_MIN,
} from './side-split-model';

function usableWidth(containerWidth: number): number {
  return Math.max(0, containerWidth - SIDE_SPLIT_SEPARATOR_W - SIDE_SPLIT_FRAME_PAD_RIGHT);
}

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

  it('caps a remembered width to half the workbench when list reserve would allow more', () => {
    const container = 1100;
    const width = clampSideSplitWidth(700, container);
    expect(width).toBe(Math.floor(usableWidth(container) * SIDE_SPLIT_MAX_SHARE));
    expect(width).toBeLessThan(700);
  });

  it('keeps a remembered width once the workbench is wide enough', () => {
    expect(clampSideSplitWidth(700, 1800)).toBe(700);
  });

  it('does not shrink a stored width before the workbench is measured', () => {
    expect(clampSideSplitWidth(700, 0)).toBe(700);
  });
});
