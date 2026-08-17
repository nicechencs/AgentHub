import * as React from 'react';
import { getSettings } from '@/lib/api/settings';
import {
  applyLanguage,
  createTranslator,
  loadStoredLanguage,
  persistLanguage,
  translate,
  type TranslateFn,
  type UiLanguage,
} from '@/lib/i18n';
import type { AppSettings } from '@/lib/types';

interface LanguageContextValue {
  lang: UiLanguage;
  setLanguage: (lang: UiLanguage) => void;
  t: TranslateFn;
}

const LanguageContext = React.createContext<LanguageContextValue>({
  lang: 'zh',
  setLanguage: () => {},
  t: (key, params) => translate('zh', key, params),
});

export function useI18n() {
  return React.useContext(LanguageContext);
}

/** 启动时应用本地语言，并与 core settings 对账。 */
export function LanguageProvider({ children }: { children: React.ReactNode }) {
  const [lang, setLangState] = React.useState<UiLanguage>(() => loadStoredLanguage());

  React.useEffect(() => {
    applyLanguage(lang);
  }, [lang]);

  React.useEffect(() => {
    let cancelled = false;
    void getSettings()
      .then((s) => {
        if (cancelled) return;
        persistLanguage(s.language);
        applyLanguage(s.language);
        setLangState(s.language);
      })
      .catch(() => {
        // Keep the local first-paint language if core is unavailable.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const setLanguage = React.useCallback((next: UiLanguage) => {
    persistLanguage(next);
    applyLanguage(next);
    setLangState(next);
  }, []);

  const t = React.useMemo(() => createTranslator(lang), [lang]);
  const value = React.useMemo(() => ({ lang, setLanguage, t }), [lang, setLanguage, t]);

  return <LanguageContext.Provider value={value}>{children}</LanguageContext.Provider>;
}

/** 设置页保存语言时同步 LanguageProvider 缓存与 html lang。 */
export function syncLanguageFromSettings(settings: Pick<AppSettings, 'language'>): void {
  persistLanguage(settings.language);
  applyLanguage(settings.language);
}
