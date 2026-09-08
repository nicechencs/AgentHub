import type { AppSettings, LogLevel } from '@/lib/types';

export interface SettingsPort {
  getSettings(): Promise<AppSettings>;
  updateSettings(patch: Partial<AppSettings>): Promise<AppSettings>;
  openLogsDir(): Promise<string>;
  /** Open http(s) URL in the system browser (Tauri cannot rely on window.open). */
  openExternalUrl(url: string): Promise<void>;
  /**
   * Open Sub2API `{base}/login` in a child webview and return session tokens
   * when localStorage is readable. Rejects with `cancelled` when the user closes
   * the window. Mock / browser builds should reject so the GUI uses paste fallback.
   */
  openSub2ApiLoginWindow(loginUrl: string): Promise<{
    accessToken: string;
    refreshToken?: string;
    expiresAt?: number;
  }>;
  /** Close the Sub2API login WebviewWindow if open (e.g. user cancelled the dialog). */
  closeSub2ApiLoginWindow(): Promise<void>;
  /**
   * Sub2API remembered-password vault JSON in SQLite settings.
   * Never log the value. Mock keeps an in-memory string.
   */
  getSub2ApiRememberedVault(): Promise<string | null>;
  setSub2ApiRememberedVault(json: string): Promise<void>;
  /**
   * Desktop HTTP for Sub2API (bypasses WebView CORS). Browser mock unused.
   * Never log Authorization / body secrets.
   */
  sub2ApiHttpRequest(input: {
    method: string;
    url: string;
    headers: Record<string, string>;
    body?: string | null;
  }): Promise<{ status: number; body: string }>;
  /** Native folder picker. `null` = cancelled. Value is a filesystem path, not a URI. */
  pickDirectory(options?: {
    title?: string;
    defaultPath?: string | null;
  }): Promise<string | null>;
  /** Native file picker. `null` = cancelled. Value is a filesystem path, not a URI. */
  pickFile(options?: {
    title?: string;
    defaultPath?: string | null;
    filters?: ReadonlyArray<{ name: string; extensions: readonly string[] }>;
  }): Promise<string | null>;
  /**
   * Best-effort GUI log line. `last4` only — never a raw key.
   * Optional `profileId` / `route` / `code` help correlate Routes actions.
   * Mock is a no-op.
   */
  logGuiEvent?(
    op: string,
    detail?: {
      agent?: string;
      last4?: string;
      profileId?: string;
      route?: string;
      code?: string;
    },
  ): Promise<void>;
  logLevelOptions: { value: LogLevel; label: string }[];
}

/** GUI log `last4`: keep a 1–4 char tail, or the last 4 of a longer secret. */
export function sanitizeGuiLast4(raw: string | null | undefined): string {
  const trimmed = (raw ?? '').trim();
  if (!trimmed || trimmed === '***') return '';
  if (trimmed.length <= 4) {
    return /^[A-Za-z0-9]+$/.test(trimmed) ? trimmed : '';
  }
  if (trimmed.length >= 8) return trimmed.slice(-4);
  return '';
}
