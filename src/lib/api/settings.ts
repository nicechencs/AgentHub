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

/** GUI log line (op + agent + last4 + optional route fields). Best-effort; never pass a raw key. */
export async function logGuiEvent(
  op: string,
  detail?: {
    agent?: string;
    last4?: string;
    profileId?: string;
    route?: string;
    code?: string;
  },
): Promise<void> {
  try {
    const port = getBackend().settings;
    if (typeof port.logGuiEvent === 'function') {
      await port.logGuiEvent(op, detail);
    }
  } catch {
    // Logging must not break the form.
  }
}

/** Stable `[code]` suffix from core/GUI error strings, when present. */
export function guiErrorCode(error: unknown): string | undefined {
  let text = '';
  if (typeof error === 'string') text = error.trim();
  else if (error instanceof Error) text = error.message.trim();
  else if (error && typeof error === 'object' && 'message' in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === 'string') text = message.trim();
  }
  const match = text.match(/\[([a-z0-9_.]+)\]\s*$/i);
  return match?.[1];
}

/** Static options (avoid module-init getBackend for tree-shaking / SSR-less safety). */
export const LOG_LEVEL_OPTIONS: { value: LogLevel; label: string }[] = [
  { value: 'error', label: 'error — 仅错误' },
  { value: 'warn', label: 'warn — 警告及以上' },
  { value: 'info', label: 'info — 常规（默认）' },
  { value: 'debug', label: 'debug — 详细诊断' },
  { value: 'trace', label: 'trace — 极细' },
];
