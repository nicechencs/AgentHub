import * as React from 'react';
import type { AppSettings } from '@/lib/types';
import { applyTheme, loadStoredTheme, persistTheme, type ThemeMode } from '@/lib/theme';

interface ThemeContextValue {
  theme: ThemeMode;
  setTheme: (mode: ThemeMode) => void;
}

const ThemeContext = React.createContext<ThemeContextValue>({
  theme: 'light',
  setTheme: () => {},
});

export function useTheme() {
  return React.useContext(ThemeContext);
}

/** 启动时应用本地主题,并监听系统偏好(theme=system 时) */
export function ThemeProvider({ children }: { children: React.ReactNode }) {
  const [theme, setThemeState] = React.useState<ThemeMode>(() => loadStoredTheme());

  React.useEffect(() => {
    applyTheme(theme);
  }, [theme]);

  React.useEffect(() => {
    if (theme !== 'system') return;
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const onChange = () => applyTheme('system');
    mq.addEventListener('change', onChange);
    return () => mq.removeEventListener('change', onChange);
  }, [theme]);

  const setTheme = React.useCallback((mode: ThemeMode) => {
    persistTheme(mode);
    setThemeState(mode);
  }, []);

  const value = React.useMemo(() => ({ theme, setTheme }), [theme, setTheme]);

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

/** 设置页保存主题时同步 ThemeProvider */
export function syncThemeFromSettings(settings: Pick<AppSettings, 'theme'>): void {
  persistTheme(settings.theme);
}
