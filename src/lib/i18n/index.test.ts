import { afterEach, describe, expect, it, vi } from 'vitest';
import { StorageKey } from '@/lib/ui-preferences';
import {
  applyLanguage,
  createTranslator,
  detectSystemLanguage,
  flattenKeys,
  htmlLang,
  interpolate,
  isLanguageSystemSeeded,
  loadStoredLanguage,
  markLanguageSystemSeeded,
  parseUiLanguage,
  persistLanguage,
  planLanguageReconcile,
  translate,
} from './index';
import { en } from './locales/en';
import { zh } from './locales/zh';

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

describe('parseUiLanguage', () => {
  it('maps core and UI forms, falling back to zh', () => {
    expect(parseUiLanguage('zh-CN')).toBe('zh');
    expect(parseUiLanguage('en-US')).toBe('en');
    expect(parseUiLanguage('en')).toBe('en');
    expect(parseUiLanguage('zh')).toBe('zh');
    expect(parseUiLanguage('nope')).toBe('zh');
    expect(parseUiLanguage(null)).toBe('zh');
  });
});

describe('translate / interpolate', () => {
  it('replaces named placeholders', () => {
    expect(interpolate('用量每 {minutes} 分钟自动采集', { minutes: 30 })).toBe(
      '用量每 30 分钟自动采集',
    );
    expect(translate('en', 'settings.data.usageAuto', { minutes: 15 })).toBe(
      'Auto-collect usage every 15 min',
    );
  });

  it('keeps missing placeholders', () => {
    expect(interpolate('hello {name}', {})).toBe('hello {name}');
  });

  it('falls back to zh then the key', () => {
    expect(translate('en', 'common.save')).toBe('Save');
    expect(translate('zh', 'common.save')).toBe('保存');
    const t = createTranslator('en');
    expect(t('nav.routes')).toBe('Routes');
    const zhNav = createTranslator('zh');
    expect(zhNav('nav.dashboard')).toBe('总览');
    expect(zhNav('nav.connections')).toBe('连接');
    expect(zhNav('nav.routes')).toBe('本机路由');
    expect(zhNav('nav.settings')).toBe('设置');
    expect(zhNav('nav.chat')).toBe('Chat');
    expect(zhNav('nav.agents')).toBe('Agents');
  });
});

describe('detectSystemLanguage', () => {
  it('picks the first recognizable zh/en from the list', () => {
    expect(detectSystemLanguage(['en-US'])).toBe('en');
    expect(detectSystemLanguage(['zh-CN'])).toBe('zh');
    expect(detectSystemLanguage(['zh_TW'])).toBe('zh');
    expect(detectSystemLanguage(['fr-FR', 'en-GB'])).toBe('en');
    expect(detectSystemLanguage(['de-DE', 'ja-JP'])).toBe('zh');
    expect(detectSystemLanguage([])).toBe('zh');
  });
});

describe('planLanguageReconcile', () => {
  it('seeds local over core once, then core wins', () => {
    expect(planLanguageReconcile('zh', 'en', false)).toEqual({ next: 'en', writeCore: true });
    expect(planLanguageReconcile('zh', 'zh', false)).toEqual({ next: 'zh', writeCore: false });
    expect(planLanguageReconcile('zh', 'en', true)).toEqual({ next: 'zh', writeCore: false });
    expect(planLanguageReconcile('en', 'zh', true)).toEqual({ next: 'en', writeCore: false });
  });
});

describe('language storage and html lang', () => {
  it('loads and persists via StorageKey.language', () => {
    stubStorage();
    persistLanguage('en');
    expect(store.get(StorageKey.language)).toBe('en');
    expect(loadStoredLanguage()).toBe('en');
  });

  it('detects system language when cache is empty and persists it', () => {
    stubStorage();
    vi.stubGlobal('navigator', { language: 'en-US', languages: ['en-US'] });
    expect(loadStoredLanguage()).toBe('en');
    expect(store.get(StorageKey.language)).toBe('en');
  });

  it('does not override an existing cache with the system language', () => {
    stubStorage();
    persistLanguage('zh');
    vi.stubGlobal('navigator', { language: 'en-US', languages: ['en-US'] });
    expect(loadStoredLanguage()).toBe('zh');
  });

  it('records the one-shot seed flag', () => {
    stubStorage();
    expect(isLanguageSystemSeeded()).toBe(false);
    markLanguageSystemSeeded();
    expect(isLanguageSystemSeeded()).toBe(true);
  });

  it('rejects illegal stored values', () => {
    stubStorage();
    store.set(StorageKey.language, 'fr');
    expect(loadStoredLanguage()).toBe('zh');
  });

  it('maps html lang and no-ops without document', () => {
    expect(htmlLang('zh')).toBe('zh-CN');
    expect(htmlLang('en')).toBe('en');
    expect(() => applyLanguage('en')).not.toThrow();
  });
});

describe('flattenKeys', () => {
  it('walks nested string leaves', () => {
    const keys = flattenKeys(zh);
    expect(keys).toContain('settings.general.languageLabel');
    expect(keys).toContain('nav.workspace');
    expect(flattenKeys(zh).sort().join('\n')).toBe(flattenKeys(en).sort().join('\n'));
  });
});
