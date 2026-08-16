/**
 * Settings API façade — core keys via backend; UI prefs via UiPreferencesStore.
 */
import { getBackend } from '@/app/runtime';
import type { AppSettings, LogLevel } from '@/lib/types';

export async function getSettings(): Promise<AppSettings> {
  return getBackend().settings.getSettings();
}

export async function updateSettings(patch: Partial<AppSettings>): Promise<AppSettings> {
  return getBackend().settings.updateSettings(patch);
}

export async function openLogsDir(): Promise<string> {
  return getBackend().settings.openLogsDir();
}

/** Open http(s) URL in the system default browser (desktop) / new tab (mock). */
export async function openExternalUrl(url: string): Promise<void> {
  return getBackend().settings.openExternalUrl(url);
}

/** Native folder picker. `null` = cancelled. */
export async function pickDirectory(options?: {
  title?: string;
  defaultPath?: string | null;
}): Promise<string | null> {
  return getBackend().settings.pickDirectory(options);
}

/** Static options (avoid module-init getBackend for tree-shaking / SSR-less safety). */
export const LOG_LEVEL_OPTIONS: { value: LogLevel; label: string }[] = [
  { value: 'error', label: 'error — 仅错误' },
  { value: 'warn', label: 'warn — 警告及以上' },
  { value: 'info', label: 'info — 常规（默认）' },
  { value: 'debug', label: 'debug — 详细诊断' },
  { value: 'trace', label: 'trace — 极细' },
];
