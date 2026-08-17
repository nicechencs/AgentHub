import { logger } from '@/lib/logger';
import { loadString, saveString, StorageKey } from '@/lib/ui-preferences';
import { en } from './locales/en';
import { zh } from './locales/zh';
import type { Dict, MessageKey, MessageParams, TranslateFn, UiLanguage } from './types';

export type { Dict, MessageKey, MessageParams, TranslateFn, UiLanguage } from './types';

const log = logger.scope('i18n');

export const DICTS: Record<UiLanguage, Dict> = { zh, en };

export function parseUiLanguage(raw: string | null | undefined): UiLanguage {
  if (!raw) return 'zh';
  const v = raw.trim().toLowerCase();
  if (v === 'en' || v.startsWith('en')) return 'en';
  if (v === 'zh' || v.startsWith('zh')) return 'zh';
  return 'zh';
}

export function htmlLang(lang: UiLanguage): 'zh-CN' | 'en' {
  return lang === 'en' ? 'en' : 'zh-CN';
}

export function flattenKeys(obj: unknown, prefix = ''): string[] {
  if (obj == null || typeof obj !== 'object') return [];
  const out: string[] = [];
  for (const [k, v] of Object.entries(obj as Record<string, unknown>)) {
    const path = prefix ? `${prefix}.${k}` : k;
    if (typeof v === 'string') out.push(path);
    else out.push(...flattenKeys(v, path));
  }
  return out;
}

function lookup(dict: unknown, key: string): string | undefined {
  let cur: unknown = dict;
  for (const part of key.split('.')) {
    if (cur == null || typeof cur !== 'object' || !(part in cur)) return undefined;
    cur = (cur as Record<string, unknown>)[part];
  }
  return typeof cur === 'string' ? cur : undefined;
}

export function interpolate(template: string, params?: MessageParams): string {
  if (!params) return template;
  return template.replace(/\{([a-zA-Z0-9_]+)\}/g, (all, name: string) => {
    if (Object.prototype.hasOwnProperty.call(params, name)) {
      return String(params[name]);
    }
    log.warn('missing i18n interpolation param', { name, template });
    return all;
  });
}

export function translate(lang: UiLanguage, key: MessageKey, params?: MessageParams): string {
  const template = lookup(DICTS[lang], key) ?? lookup(DICTS.zh, key);
  if (template == null) {
    log.warn('missing i18n key', { lang, key });
    return key;
  }
  return interpolate(template, params);
}

export function createTranslator(lang: UiLanguage): TranslateFn {
  return (key, params) => translate(lang, key, params);
}

export function loadStoredLanguage(): UiLanguage {
  return parseUiLanguage(loadString(StorageKey.language, 'zh'));
}

export function persistLanguage(lang: UiLanguage): void {
  saveString(StorageKey.language, lang);
}

export function applyLanguage(lang: UiLanguage): void {
  if (typeof document === 'undefined') return;
  document.documentElement.lang = htmlLang(lang);
}
