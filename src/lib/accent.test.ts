import { afterEach, describe, expect, it, vi } from 'vitest';
import { ACCENT_IDS, DEFAULT_ACCENT_ID, isAccentId } from '@/styles/tokens';
import { StorageKey } from '@/lib/ui-preferences';
import { applyAccent, loadStoredAccent, persistAccent, registerShellIconSync } from './accent';

const store = new Map<string, string>();
const html = {
  dataset: {} as Record<string, string>,
  removeAttribute(name: string) {
    if (name === 'data-accent') delete this.dataset.accent;
  },
};

afterEach(() => {
  store.clear();
  html.dataset = {};
  registerShellIconSync(null);
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

describe('accent preference', () => {
  it('defaults to indigo and rejects unknown ids', () => {
    stubDom();
    expect(loadStoredAccent()).toBe(DEFAULT_ACCENT_ID);
    expect(isAccentId('indigo')).toBe(true);
    expect(isAccentId('navy')).toBe(false);
  });

  it('applies data-accent and persists a known id', () => {
    stubDom();
    persistAccent('teal');
    expect(html.dataset.accent).toBe('teal');
    expect(store.get(StorageKey.accent)).toBe('teal');
    expect(loadStoredAccent()).toBe('teal');
  });

  it('falls back when storage holds an unknown id', () => {
    stubDom();
    store.set(StorageKey.accent, 'navy');
    expect(loadStoredAccent()).toBe(DEFAULT_ACCENT_ID);
    applyAccent('blue');
    expect(html.dataset.accent).toBe('blue');
  });

  it('notifies the registered shell-icon hook', () => {
    stubDom();
    const sync = vi.fn();
    registerShellIconSync(sync);
    persistAccent('rose');
    expect(sync).toHaveBeenCalledWith('rose');
  });

  it('covers every palette id', () => {
    stubDom();
    expect(ACCENT_IDS).toEqual(['indigo', 'blue', 'teal', 'rose', 'amber']);
    for (const id of ACCENT_IDS) {
      persistAccent(id);
      expect(loadStoredAccent()).toBe(id);
      expect(html.dataset.accent).toBe(id);
    }
  });
});
