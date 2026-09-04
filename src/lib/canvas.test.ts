import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  CANVAS_IDS,
  CANVAS_PALETTES,
  DEFAULT_CANVAS_ID,
  isCanvasId,
} from '@/styles/tokens';
import { StorageKey } from '@/lib/ui-preferences';
import { applyCanvas, loadStoredCanvas, persistCanvas } from './canvas';

const store = new Map<string, string>();
const html = {
  dataset: {} as Record<string, string>,
};

afterEach(() => {
  store.clear();
  html.dataset = {};
  vi.unstubAllGlobals();
});

function stubDom() {
  vi.stubGlobal('localStorage', {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => {
      store.set(key, value);
    },
    removeItem: (key: string) => {
      store.delete(key);
    },
  });
  vi.stubGlobal('document', { documentElement: html });
}

function hexLuminance(hex: string): number {
  const n = hex.replace('#', '');
  const toLin = (channel: string) => {
    const c = Number.parseInt(channel, 16) / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * toLin(n.slice(0, 2)) + 0.7152 * toLin(n.slice(2, 4)) + 0.0722 * toLin(n.slice(4, 6));
}

describe('canvas preference', () => {
  it('defaults to gray and rejects unknown ids', () => {
    stubDom();
    expect(loadStoredCanvas()).toBe(DEFAULT_CANVAS_ID);
    expect(isCanvasId('sand')).toBe(true);
    expect(isCanvasId('navy')).toBe(false);
  });

  it('applies data-canvas and persists a known id', () => {
    stubDom();
    persistCanvas('sand');
    expect(html.dataset.canvas).toBe('sand');
    expect(store.get(StorageKey.canvas)).toBe('sand');
    expect(loadStoredCanvas()).toBe('sand');
  });

  it('falls back when storage holds an unknown id', () => {
    stubDom();
    store.set(StorageKey.canvas, 'navy');
    expect(loadStoredCanvas()).toBe(DEFAULT_CANVAS_ID);
    applyCanvas('mint');
    expect(html.dataset.canvas).toBe('mint');
  });

  it('keeps every swatch light', () => {
    stubDom();
    expect(CANVAS_IDS.length).toBeGreaterThanOrEqual(6);
    for (const id of CANVAS_IDS) {
      persistCanvas(id);
      expect(loadStoredCanvas()).toBe(id);
      expect(hexLuminance(CANVAS_PALETTES[id].canvas)).toBeGreaterThan(0.8);
      expect(hexLuminance(CANVAS_PALETTES[id].subtle)).toBeGreaterThan(0.75);
    }
  });
});
