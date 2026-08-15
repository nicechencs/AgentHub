import { getVersion } from '@tauri-apps/api/app';
import type { SettingsPort } from '@/lib/backend/contracts';
import { UNKNOWN_APP_VERSION } from '@/lib/app-version';
import { logger } from '@/lib/logger';
import {
  loadBool,
  loadJson,
  loadString,
  saveBool,
  saveJson,
  saveString,
  StorageKey,
} from '@/lib/ui-preferences';
import { applyTheme, type ThemeMode } from '@/lib/theme';
import type { AppSettings, LogLevel, SkillMarketSource } from '@/lib/types';
import { invoke } from './invoke';
import { normalizeIntervalMin } from '@/lib/usage-sync';

/** OS login-item helpers (Tauri plugin). Soft-fail outside desktop shell. */
async function readOsAutoStart(): Promise<boolean | undefined> {
  try {
    const { isEnabled } = await import('@tauri-apps/plugin-autostart');
    return await isEnabled();
  } catch {
    return undefined;
  }
}

/**
 * auto-launch 0.5 on Windows: `disable()` calls `delete_value` and fails with
 * ERROR_FILE_NOT_FOUND (os error 2 / 系统找不到指定的文件) when the Run key
 * entry is already absent. Treat that as already-disabled success.
 */
export function isAlreadyDisabledAutostartError(e: unknown): boolean {
  const msg = e instanceof Error ? e.message : String(e);
  return /os error 2|ERROR_FILE_NOT_FOUND|找不到指定的文件|cannot find the file|not found/i.test(
    msg,
  );
}

/**
 * Write OS login-item. Skips when already at the desired state so saving other
 * general settings does not re-touch the registry. Disable is idempotent on
 * Windows (missing Run value is success).
 */
async function writeOsAutoStart(enabled: boolean): Promise<void> {
  const { enable, disable, isEnabled } = await import('@tauri-apps/plugin-autostart');
  try {
    if ((await isEnabled()) === enabled) return;
  } catch {
    // Plugin probe failed — still attempt the write below.
  }
  if (enabled) {
    await enable();
    return;
  }
  try {
    await disable();
  } catch (e) {
    if (isAlreadyDisabledAutostartError(e)) return;
    throw e;
  }
}

const log = logger.scope('backend:tauri:settings');
const LOG_LEVELS: LogLevel[] = ['error', 'warn', 'info', 'debug', 'trace'];
const SKILL_MARKET_SOURCES: SkillMarketSource[] = ['auto', 'skills.sh', 'skillhub.cn'];

const DEFAULTS: AppSettings = {
  language: 'zh',
  theme: 'light',
  autoStart: true,
  closeToTray: true,
  dataDir: '~/.agenthub',
  logsDir: '~/.agenthub/logs',
  logLevel: 'info',
  logRetentionDays: 14,
  skillMarketSource: 'auto',
  usageCollectIntervalMin: 30,
  // Real value comes from Tauri getVersion(); do not hardcode a product semver.
  appVersion: UNKNOWN_APP_VERSION,
};

const SETTINGS_KEY = 'agenthub:settings';

interface CoreAppSettings {
  theme: string;
  language: string;
  logLevel: string;
  logRetentionDays: number;
  skillMarketSource?: string;
  closeToTray?: boolean;
  /** Foreground usage collect interval (minutes). Omitted on older cores. */
  usageCollectIntervalMin?: number;
}

interface CorePathInfo {
  dataDir: string;
  dbPath: string;
  backupsDir: string;
  logsDir: string;
}

function parseLogLevel(raw: string | undefined | null): LogLevel {
  const v = (raw ?? 'info').trim().toLowerCase();
  return (LOG_LEVELS as string[]).includes(v) ? (v as LogLevel) : 'info';
}

function parseSkillMarketSource(raw: string | undefined | null): SkillMarketSource {
  const v = (raw ?? 'auto').trim().toLowerCase();
  if (v === 'skills.sh' || v === 'skills_sh' || v === 'skillssh') return 'skills.sh';
  if (v === 'skillhub.cn' || v === 'skillhub' || v === 'skillhub_cn') return 'skillhub.cn';
  if ((SKILL_MARKET_SOURCES as string[]).includes(v)) return v as SkillMarketSource;
  return 'auto';
}

function mapLanguageToUi(raw: string): AppSettings['language'] {
  const l = raw.toLowerCase();
  if (l.startsWith('en')) return 'en';
  return 'zh';
}

function mapLanguageToCore(ui: AppSettings['language']): string {
  return ui === 'en' ? 'en' : 'zh-CN';
}

function mapTheme(raw: string): ThemeMode {
  if (raw === 'light' || raw === 'dark' || raw === 'system') return raw;
  return 'system';
}

/**
 * Resolve close-to-tray for the settings UI.
 * Core (Rust) is authoritative; localStorage is a fallback for older clients.
 */
export function resolveCloseToTray(
  core: boolean | undefined,
  local: boolean | undefined,
  fallback = DEFAULTS.closeToTray,
): boolean {
  if (typeof core === 'boolean') return core;
  if (typeof local === 'boolean') return local;
  return fallback;
}

/** Serialize close-to-tray for `set_setting` (core whitelist expects true/false). */
export function closeToTraySettingValue(enabled: boolean): string {
  return enabled ? 'true' : 'false';
}

/**
 * Resolve usage collect interval (minutes).
 * Core is authoritative; localStorage is a one-shot migration fallback.
 */
export function resolveUsageCollectIntervalMin(
  core: number | undefined,
  local: number | undefined,
  fallback = DEFAULTS.usageCollectIntervalMin,
): number {
  if (typeof core === 'number' && Number.isFinite(core)) return core;
  if (typeof local === 'number' && Number.isFinite(local)) return local;
  return fallback;
}

/** UI-only fields that still live in localStorage (not mock business data). */
function loadUiLocal(): Partial<AppSettings> {
  return loadJson<Partial<AppSettings>>(SETTINGS_KEY, {});
}

const LOG_LEVEL_OPTIONS: { value: LogLevel; label: string }[] = [
  { value: 'error', label: 'error — 仅错误' },
  { value: 'warn', label: 'warn — 警告及以上' },
  { value: 'info', label: 'info — 常规（默认）' },
  { value: 'debug', label: 'debug — 详细诊断' },
  { value: 'trace', label: 'trace — 极细' },
];

export function createTauriSettingsPort(): SettingsPort {
  return {
    logLevelOptions: LOG_LEVEL_OPTIONS,

    async getSettings() {
      try {
        const [core, paths, appVersion, osAutoStart] = await Promise.all([
          invoke<CoreAppSettings>('get_app_settings'),
          invoke<CorePathInfo>('get_path_info'),
          getVersion().catch(() => DEFAULTS.appVersion),
          readOsAutoStart(),
        ]);
        const local = loadUiLocal();
        // Core theme is authoritative; localStorage is a first-paint cache only.
        const theme = mapTheme(core.theme ?? loadString(StorageKey.theme, DEFAULTS.theme));
        saveString(StorageKey.theme, theme);

        let usageCollectIntervalMin = resolveUsageCollectIntervalMin(
          core.usageCollectIntervalMin,
          local.usageCollectIntervalMin,
        );
        // Key absent in SQLite (Option/undefined): migrate local once, else default.
        if (typeof core.usageCollectIntervalMin !== 'number') {
          const localInterval = local.usageCollectIntervalMin;
          if (typeof localInterval === 'number' && Number.isFinite(localInterval)) {
            const migrated = normalizeIntervalMin(localInterval);
            usageCollectIntervalMin = migrated;
            if (!loadBool(StorageKey.usageIntervalMigrated, false)) {
              try {
                await invoke('set_setting', {
                  key: 'usage_collect_interval_min',
                  value: String(migrated),
                });
                saveBool(StorageKey.usageIntervalMigrated, true);
              } catch (e) {
                log.error('migrate usageCollectIntervalMin to core failed', e);
              }
            }
          } else {
            usageCollectIntervalMin = DEFAULTS.usageCollectIntervalMin;
            saveBool(StorageKey.usageIntervalMigrated, true);
          }
        } else {
          saveBool(StorageKey.usageIntervalMigrated, true);
        }
        const next: AppSettings = {
          ...DEFAULTS,
          theme,
          language: mapLanguageToUi(core.language),
          logLevel: parseLogLevel(core.logLevel),
          logRetentionDays: core.logRetentionDays || 14,
          skillMarketSource: parseSkillMarketSource(core.skillMarketSource),
          // Core is source of truth for close-to-tray (Rust window handler reads it).
          closeToTray: resolveCloseToTray(core.closeToTray, local.closeToTray),
          // OS login item is authoritative when the plugin is available.
          autoStart:
            typeof osAutoStart === 'boolean'
              ? osAutoStart
              : typeof local.autoStart === 'boolean'
                ? local.autoStart
                : DEFAULTS.autoStart,
          dataDir: paths.dataDir,
          logsDir: paths.logsDir,
          usageCollectIntervalMin,
          // Package version from Tauri shell (not localStorage).
          appVersion: appVersion || DEFAULTS.appVersion,
        };
        applyTheme(next.theme);
        return { ...next };
      } catch (e) {
        log.error('getSettings failed', e);
        throw e;
      }
    },

    async updateSettings(patch) {
      try {
        if (patch.theme !== undefined) {
          await invoke('set_setting', { key: 'theme', value: patch.theme });
          saveString(StorageKey.theme, patch.theme);
          applyTheme(patch.theme);
        }
        if (patch.language !== undefined) {
          await invoke('set_setting', {
            key: 'language',
            value: mapLanguageToCore(patch.language),
          });
          saveString(StorageKey.language, patch.language);
        }
        if (patch.logLevel !== undefined) {
          await invoke('set_setting', { key: 'log_level', value: patch.logLevel });
        }
        if (patch.logRetentionDays !== undefined) {
          await invoke('set_setting', {
            key: 'log_retention_days',
            value: String(patch.logRetentionDays),
          });
        }
        if (patch.skillMarketSource !== undefined) {
          await invoke('set_setting', {
            key: 'skill_market_source',
            value: patch.skillMarketSource,
          });
        }
        if (patch.closeToTray !== undefined) {
          await invoke('set_setting', {
            key: 'close_to_tray',
            value: closeToTraySettingValue(patch.closeToTray),
          });
        }
        if (patch.usageCollectIntervalMin !== undefined) {
          const mins = normalizeIntervalMin(patch.usageCollectIntervalMin);
          await invoke('set_setting', {
            key: 'usage_collect_interval_min',
            value: String(mins),
          });
          saveBool(StorageKey.usageIntervalMigrated, true);
          patch = { ...patch, usageCollectIntervalMin: mins };
        }
        // OS login-item last so a failure does not roll back already-written core keys.
        if (patch.autoStart !== undefined) {
          try {
            await writeOsAutoStart(patch.autoStart);
          } catch (e) {
            log.error('OS autostart update failed', e);
            throw new Error(
              `无法写入系统开机自启：${e instanceof Error ? e.message : String(e)}`,
            );
          }
        }

        const local = loadUiLocal();
        // Only persist UI-local fields. Leftover keys (hasMasterPassword /
        // credentialStore / autoBackup) from older clients are ignored.
        const mergedLocal = {
          autoStart: patch.autoStart ?? local.autoStart ?? DEFAULTS.autoStart,
          // Mirror for offline UI; core DB is authoritative after successful set.
          closeToTray: patch.closeToTray ?? local.closeToTray ?? DEFAULTS.closeToTray,
          usageCollectIntervalMin:
            patch.usageCollectIntervalMin ??
            local.usageCollectIntervalMin ??
            DEFAULTS.usageCollectIntervalMin,
        };
        saveJson(SETTINGS_KEY, mergedLocal);

        return await this.getSettings();
      } catch (e) {
        log.error('updateSettings failed', e);
        throw e;
      }
    },

    async openLogsDir() {
      try {
        return await invoke<string>('open_logs_dir');
      } catch (e) {
        log.error('openLogsDir failed', e);
        throw e;
      }
    },

    async openExternalUrl(url) {
      try {
        await invoke('open_external_url', { url });
      } catch (e) {
        log.error('openExternalUrl failed', e);
        throw e;
      }
    },
  };
}
