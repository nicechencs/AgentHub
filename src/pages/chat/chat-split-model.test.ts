import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  clampComposerPaneHeight,
  COMPOSER_LINE_PX,
  COMPOSER_MAX_SHARE,
  COMPOSER_PANE_HEIGHT_STORAGE_KEY,
  COMPOSER_PANE_MIN,
  COMPOSER_SPLIT_MIN_LINES,
  COMPOSER_TWO_LINE_CONTENT_PX,
  composerPaneMaxHeight,
  persistComposerPaneHeight,
  readStoredComposerPaneHeight,
} from './chat-split-model';
import { StorageKey } from '@/lib/storage-key';

describe('clampComposerPaneHeight', () => {
  it('sizes the drag floor from two body line boxes including leading', () => {
    expect(COMPOSER_TWO_LINE_CONTENT_PX).toBe(COMPOSER_LINE_PX * COMPOSER_SPLIT_MIN_LINES);
    expect(COMPOSER_PANE_MIN).toBeGreaterThan(COMPOSER_TWO_LINE_CONTENT_PX);
  });

  it('keeps a mid-range composer when the stage is tall', () => {
    expect(clampComposerPaneHeight(200, 800)).toBe(200);
  });

  it('does not shrink below two line-heights plus composer chrome', () => {
    expect(clampComposerPaneHeight(40, 800)).toBe(COMPOSER_PANE_MIN);
  });

  it('does not grow past half the stage when dragging up', () => {
    expect(clampComposerPaneHeight(4000, 800)).toBe(composerPaneMaxHeight(800));
    expect(composerPaneMaxHeight(800)).toBe(Math.floor(800 * COMPOSER_MAX_SHARE));
  });

  it('stays at half when the two-line min would exceed half', () => {
    const stage = COMPOSER_PANE_MIN * 2 - 20;
    const height = clampComposerPaneHeight(40, stage);
    expect(height).toBe(composerPaneMaxHeight(stage));
    expect(height).toBeLessThan(COMPOSER_PANE_MIN);
  });

  it('passes through an unmeasured stage without collapsing below two lines', () => {
    expect(clampComposerPaneHeight(180, 0)).toBe(180);
    expect(clampComposerPaneHeight(20, 0)).toBe(COMPOSER_PANE_MIN);
  });
});

describe('composer pane height persistence', () => {
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

  beforeEach(() => {
    store.clear();
    vi.stubGlobal('window', { localStorage });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('returns null when nothing is stored', () => {
    expect(readStoredComposerPaneHeight()).toBeNull();
  });

  it('round-trips a positive height and clears on null', () => {
    persistComposerPaneHeight(220);
    expect(readStoredComposerPaneHeight()).toBe(220);
    expect(store.get(StorageKey.chatComposerPaneHeight)).toBe('220');
    persistComposerPaneHeight(null);
    expect(readStoredComposerPaneHeight()).toBeNull();
  });

  it('clears the canonical key', () => {
    store.set(StorageKey.chatComposerPaneHeight, '220');
    persistComposerPaneHeight(null);
    expect(readStoredComposerPaneHeight()).toBeNull();
    expect(store.has(StorageKey.chatComposerPaneHeight)).toBe(false);
  });

  it('ignores non-numeric storage', () => {
    localStorage.setItem(COMPOSER_PANE_HEIGHT_STORAGE_KEY, 'nope');
    expect(readStoredComposerPaneHeight()).toBeNull();
  });
});
