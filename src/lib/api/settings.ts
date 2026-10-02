/**
 * Settings API façade — core keys via backend; UI prefs via UiPreferencesStore.
 */
import { getBackend } from '@/app/runtime';
import { sanitizeGuiLast4 } from '@/lib/backend/contracts/settings-port';
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

/** Native file picker. `null` = cancelled. */
export async function pickFile(options?: {
  title?: string;
  defaultPath?: string | null;
  filters?: ReadonlyArray<{ name: string; extensions: readonly string[] }>;
}): Promise<string | null> {
  return getBackend().settings.pickFile(options);
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
      await port.logGuiEvent(op, {
        ...detail,
        last4: sanitizeGuiLast4(detail?.last4),
      });
    }
  } catch {
    // Logging must not break the form.
  }
}

/** Stable `[code]` suffix from core/GUI error strings, when present. */
export function guiErrorCode(error: unknown): string | undefined {
  if (error && typeof error === 'object' && 'code' in error) {
    const code = (error as { code: unknown }).code;
    const structured = safeGuiErrorCode(code);
    if (structured) return structured;
  }

  let text = '';
  if (typeof error === 'string') text = error.trim();
  else if (error instanceof Error) text = error.message.trim();
  else if (error && typeof error === 'object' && 'message' in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === 'string') text = message.trim();
  }
  const match = text.match(/\[([a-z0-9_.]+)\]\s*$/i);
  return safeGuiErrorCode(match?.[1]);
}

const SIMPLE_GUI_ERROR_CODES = new Set([
  'db',
  'env_not_ready',
  'invalid_arg',
  'io',
  'json',
  'needs_attention',
  'not_found',
  'unsupported',
]);

const TRUSTED_GUI_ERROR_NAMESPACES = new Set([
  'account',
  'adapter',
  'agent',
  'backend',
  'connection',
  'config',
  'env',
  'install',
  'kiro',
  'mcp',
  'oauth',
  'paths',
  'plugin',
  'project',
  'provider',
  'route',
  'route_pool',
  'run',
  'settings',
  'skill',
  'sub2api',
  'ticket',
]);

/** Keep diagnostic codes machine-readable without ever treating arbitrary text as a code. */
function safeGuiErrorCode(raw: unknown): string | undefined {
  if (typeof raw !== 'string') return undefined;
  const code = raw.trim();
  if (!code || code.length > 120) return undefined;
  if (SIMPLE_GUI_ERROR_CODES.has(code.toLowerCase())) return code;
  const namespaced = code.replace(/^retryable:/i, '').toLowerCase();
  if (!/^[a-z][a-z0-9_-]*(?:\.[a-z0-9_.-]+)+$/i.test(namespaced)) return undefined;
  const namespace = namespaced.split('.')[0];
  return TRUSTED_GUI_ERROR_NAMESPACES.has(namespace) ? code : undefined;
}

/** Static options (avoid module-init getBackend for tree-shaking / SSR-less safety). */
export const LOG_LEVEL_OPTIONS: { value: LogLevel; label: string }[] = [
  { value: 'error', label: 'error — 仅错误' },
  { value: 'warn', label: 'warn — 警告及以上' },
  { value: 'info', label: 'info — 常规（默认）' },
  { value: 'debug', label: 'debug — 详细诊断' },
  { value: 'trace', label: 'trace — 极细' },
];
