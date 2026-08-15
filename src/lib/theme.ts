import type { AppSettings } from '@/lib/types';
import { loadString, saveString, StorageKey } from '@/lib/ui-preferences';

export type ThemeMode = AppSettings['theme'];

/** 解析 system 主题为实际 dark/light(默认偏浅色) */
export function resolveTheme(mode: ThemeMode): 'dark' | 'light' {
  if (mode === 'system') {
    if (typeof window !== 'undefined' && window.matchMedia('(prefers-color-scheme: dark)').matches) {
      return 'dark';
    }
    return 'light';
  }
  return mode;
}

/**
 * 将主题写到 <html> class。
 * 默认浅色(:root),深色时加 .dark(见 globals.css)。
 */
export function applyTheme(mode: ThemeMode): void {
  if (typeof document === 'undefined') return;
  const resolved = resolveTheme(mode);
  document.documentElement.classList.toggle('dark', resolved === 'dark');
  document.documentElement.classList.toggle('light', resolved === 'light');
  document.documentElement.dataset.theme = mode;
}

/** 浅色主题迁移标记:旧版默认 dark,本次改为白底主调,仅迁移一次 */
const THEME_MIGRATION = 'agenthub:theme-v2-light';

export function loadStoredTheme(): ThemeMode {
  try {
    if (!localStorage.getItem(THEME_MIGRATION)) {
      localStorage.setItem(THEME_MIGRATION, '1');
      // 首次进入 v2:强制切到浅色白底(用户之后仍可在设置里改)
      saveString(StorageKey.theme, 'light');
      return 'light';
    }
  } catch {
    // ignore
  }
  const v = loadString(StorageKey.theme, 'light');
  if (v === 'light' || v === 'system' || v === 'dark') return v;
  return 'light';
}

export function persistTheme(mode: ThemeMode): void {
  saveString(StorageKey.theme, mode);
  applyTheme(mode);
}
