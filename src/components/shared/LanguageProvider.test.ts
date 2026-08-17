import { afterEach, describe, expect, it, vi } from 'vitest';
import { StorageKey } from '@/lib/ui-preferences';
import { applyLanguage, loadStoredLanguage, persistLanguage } from '@/lib/i18n';
import { syncLanguageFromSettings } from './LanguageProvider';

const store = new Map<string, string>();

afterEach(() => {
  store.clear();
  vi.unstubAllGlobals();
});

function stubStorage() {
  vi.stubGlobal('localStorage', {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => {
      store.set(key, value);
    },
    removeItem: (key: string) => {
      store.delete(key);
    },
  });
}

describe('LanguageProvider helpers', () => {
  it('first paint reads the language cache', () => {
    stubStorage();
    persistLanguage('en');
    expect(loadStoredLanguage()).toBe('en');
  });

  it('syncLanguageFromSettings writes cache and is safe without document', () => {
    stubStorage();
    expect(() => syncLanguageFromSettings({ language: 'en' })).not.toThrow();
    expect(store.get(StorageKey.language)).toBe('en');
    expect(() => applyLanguage('zh')).not.toThrow();
  });

  it('keeps cached language when core reconcile is skipped', () => {
    stubStorage();
    persistLanguage('en');
    // getSettings failure path: caller keeps loadStoredLanguage().
    expect(loadStoredLanguage()).toBe('en');
  });
});
