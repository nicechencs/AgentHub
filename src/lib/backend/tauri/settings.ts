import { getVersion } from '@tauri-apps/api/app';
import type { SettingsPort } from '@/lib/backend/contracts';
import { logger } from '@/lib/logger';
import {
  loadJson,
  loadString,
  saveJson,
  saveString,
  StorageKey,
} from '@/lib/ui-preferences';
import { applyTheme, type ThemeMode } from '@/lib/theme';
import type { AppSettings, LogLevel, SkillMarketSource } from '@/lib/types';
import { invoke } from './invoke';

const log = logger.scope('backend:tauri:settings');
const LOG_LEVELS: LogLevel[] = ['error', 'warn', 'info', 'debug', 'trace'];
const SKILL_MARKET_SOURCES: SkillMarketSource[] = ['auto', 'skills.sh', 'skillhub.cn'];

const DEFAULTS: AppSettings = {
  language: 'zh',
  theme: 'light',
  autoStart: true,
  closeToTray: true,
  hasMasterPassword: false,
  credentialStore: 'keyring',
  dataDir: '~/.agenthub',
  logsDir: '~/.agenthub/logs',
  logLevel: 'info',
  logRetentionDays: 14,
  skillMarketSource: 'auto',
  autoBackup: true,
  usageCollectIntervalMin: 30,
  appVersion: '0.1.0',
};

const SETTINGS_KEY = 'agenthub:settings';

interface CoreAppSettings {
  theme: string;
  language: string;
  logLevel: string;
  logRetentionDays: number;
  skillMarketSource?: string;
  closeToTray?: boolean;
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
        const [core, paths, appVersion] = await Promise.all([
          invoke<CoreAppSettings>('get_app_settings'),
          invoke<CorePathInfo>('get_path_info'),
          getVersion().catch(() => DEFAULTS.appVersion),
        ]);
        const local = loadUiLocal();
        const themeRaw = loadString(StorageKey.theme, core.theme ?? DEFAULTS.theme);
        const next: AppSettings = {
          ...DEFAULTS,
          ...local,
          theme: mapTheme(themeRaw),
          language: mapLanguageToUi(core.language),
          logLevel: parseLogLevel(core.logLevel),
          logRetentionDays: core.logRetentionDays || 14,
          skillMarketSource: parseSkillMarketSource(core.skillMarketSource),
          // Core is source of truth for close-to-tray (Rust window handler reads it).
          closeToTray: resolveCloseToTray(core.closeToTray, local.closeToTray),
          dataDir: paths.dataDir,
          logsDir: paths.logsDir,
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

        const local = loadUiLocal();
        const mergedLocal = {
          ...local,
          autoStart: patch.autoStart ?? local.autoStart ?? DEFAULTS.autoStart,
          // Mirror for offline UI; core DB is authoritative after successful set.
          closeToTray: patch.closeToTray ?? local.closeToTray ?? DEFAULTS.closeToTray,
          hasMasterPassword:
            patch.hasMasterPassword ?? local.hasMasterPassword ?? DEFAULTS.hasMasterPassword,
          credentialStore:
            patch.credentialStore ?? local.credentialStore ?? DEFAULTS.credentialStore,
          autoBackup: patch.autoBackup ?? local.autoBackup ?? DEFAULTS.autoBackup,
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
