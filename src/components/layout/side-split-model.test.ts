import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { StorageKey } from '@/lib/storage-key';
import {
  clampSideSplitWidth,
  persistSideSplitWidth,
  readStoredSideSplitWidth,
  SIDE_SPLIT_FRAME_PAD_X,
  SIDE_SPLIT_FRAME_PAD_X_FLUSH,
  SIDE_SPLIT_MAIN_MIN,
  SIDE_SPLIT_MAIN_FLOOR,
  SIDE_SPLIT_MAX_SHARE,
  SIDE_SPLIT_SEPARATOR_W,
  SIDE_SPLIT_WIDTH_DEFAULT,
  SIDE_SPLIT_WIDTH_FLOOR,
  SIDE_SPLIT_WIDTH_MIN,
} from './side-split-model';

const dir = path.dirname(fileURLToPath(import.meta.url));

function source(name: string): string {
  return readFileSync(path.join(dir, name), 'utf8');
}

function usableWidth(containerWidth: number): number {
  return Math.max(0, containerWidth - SIDE_SPLIT_SEPARATOR_W - SIDE_SPLIT_FRAME_PAD_X * 2);
}

describe('clampSideSplitWidth', () => {
  it('keeps a default-sized pane when the workbench is wide', () => {
    expect(clampSideSplitWidth(SIDE_SPLIT_WIDTH_DEFAULT, 1200)).toBe(SIDE_SPLIT_WIDTH_DEFAULT);
  });

  it('prioritizes the list column over the pane floor in a narrow workbench', () => {
    const width = clampSideSplitWidth(120, 400);
    expect(width).toBeLessThan(SIDE_SPLIT_WIDTH_FLOOR);
    expect(width).toBeLessThanOrEqual(usableWidth(400) - SIDE_SPLIT_MAIN_FLOOR);
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

  it('lets the pane grow past half the workbench while the list still has room', () => {
    const container = 1200;
    const half = Math.floor(usableWidth(container) * 0.5);
    const width = clampSideSplitWidth(700, container);
    expect(width).toBe(700);
    expect(width).toBeGreaterThan(half);
  });

  it('caps a remembered width to the max share when list reserve would allow more', () => {
    const container = 2000;
    const width = clampSideSplitWidth(1600, container);
    expect(width).toBe(Math.floor(usableWidth(container) * SIDE_SPLIT_MAX_SHARE));
    expect(width).toBeLessThan(1600);
  });

  it('keeps a remembered width once the workbench is wide enough', () => {
    expect(clampSideSplitWidth(700, 1800)).toBe(700);
  });

  it('does not shrink a stored width before the workbench is measured', () => {
    expect(clampSideSplitWidth(700, 0)).toBe(700);
  });
});

describe('side-split frame pad', () => {
  it('keeps the page inset for card-hosted workbenches and lets Chat opt out', () => {
    expect(SIDE_SPLIT_FRAME_PAD_X).toBe(12);
    expect(SIDE_SPLIT_FRAME_PAD_X_FLUSH).toBe(0);
  });

  it('reads one pad from the controller in both the hook and the frame', () => {
    const hook = source('use-side-split.ts');
    const frame = source('SideSplit.tsx');
    expect(hook).toContain('const framePadX = options.framePadX ?? SIDE_SPLIT_FRAME_PAD_X;');
    expect(hook).toContain('paneWidth + framePadX * 2');
    expect(frame).toContain('width: split.paneWidth + split.framePadX * 2');
    expect(frame).toContain('paddingLeft: split.framePadX');
    expect(frame).toContain('paddingRight: split.framePadX');
  });
});

describe('side-split width persistence', () => {
  const store = new Map<string, string>();
  const localStorage = {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => {
      store.set(key, value);
    },
    removeItem: (key: string) => {
      store.delete(key);
    },
  };

  afterEach(() => {
    store.clear();
    vi.unstubAllGlobals();
  });

  function stubWindow() {
    vi.stubGlobal('window', { localStorage });
  }

  it('writes and reads the canonical inspect width', () => {
    stubWindow();
    persistSideSplitWidth(StorageKey.connectionsInspectWidth, 520);
    expect(store.get(StorageKey.connectionsInspectWidth)).toBe('520');
    expect(readStoredSideSplitWidth(StorageKey.connectionsInspectWidth)).toBe(520);
  });
});
