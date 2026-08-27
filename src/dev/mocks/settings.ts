import type { SettingsPort } from '@/lib/backend/contracts';
import { delay, randomLatency } from '@/dev/mocks/delay';
import {
  loadJson,
  loadString,
  saveJson,
  saveString,
  StorageKey,
} from '@/lib/ui-preferences';
import { packageAppVersion } from '@/lib/app-version';
import { applyTheme, type ThemeMode } from '@/lib/theme';
import type { AppSettings, LogLevel, SkillMarketSource } from '@/lib/types';

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
  // Tracks package.json via Vite inject — no hand-maintained semver.
  appVersion: packageAppVersion(),
};

const SETTINGS_KEY = 'agenthub:settings';

function parseLogLevel(raw: string | undefined | null): LogLevel {
  const v = (raw ?? 'info').trim().toLowerCase();
  return (LOG_LEVELS as string[]).includes(v) ? (v as LogLevel) : 'info';
}

function parseSkillMarketSource(raw: string | undefined | null): SkillMarketSource {
  const v = (raw ?? 'auto').trim().toLowerCase();
  return (SKILL_MARKET_SOURCES as string[]).includes(v) ? (v as SkillMarketSource) : 'auto';
}

function mapTheme(raw: string): ThemeMode {
  if (raw === 'light' || raw === 'dark' || raw === 'system') return raw;
  return 'system';
}

function loadState(): AppSettings {
  const stored = loadJson<Partial<AppSettings>>(SETTINGS_KEY, {});
  // Blob is the mock's durable store (core analogue); StorageKey.theme is cache.
  const theme = mapTheme(stored.theme ?? loadString(StorageKey.theme, DEFAULTS.theme));
  const logLevel = parseLogLevel(stored.logLevel ?? DEFAULTS.logLevel);
  const logRetentionDays =
    typeof stored.logRetentionDays === 'number' && stored.logRetentionDays >= 1
      ? Math.min(365, Math.floor(stored.logRetentionDays))
      : DEFAULTS.logRetentionDays;
  const skillMarketSource = parseSkillMarketSource(
    stored.skillMarketSource ?? DEFAULTS.skillMarketSource,
  );
  return {
    ...DEFAULTS,
    language: stored.language === 'en' || stored.language === 'zh' ? stored.language : DEFAULTS.language,
    theme,
    autoStart: typeof stored.autoStart === 'boolean' ? stored.autoStart : DEFAULTS.autoStart,
    closeToTray: typeof stored.closeToTray === 'boolean' ? stored.closeToTray : DEFAULTS.closeToTray,
    dataDir: stored.dataDir ?? DEFAULTS.dataDir,
    logsDir: stored.logsDir ?? DEFAULTS.logsDir,
    logLevel,
    logRetentionDays,
    skillMarketSource,
    usageCollectIntervalMin:
      typeof stored.usageCollectIntervalMin === 'number'
        ? stored.usageCollectIntervalMin
        : DEFAULTS.usageCollectIntervalMin,
    appVersion: stored.appVersion ?? DEFAULTS.appVersion,
  };
}

let state: AppSettings = loadState();

const LOG_LEVEL_OPTIONS: { value: LogLevel; label: string }[] = [
  { value: 'error', label: 'error — 仅错误' },
  { value: 'warn', label: 'warn — 警告及以上' },
  { value: 'info', label: 'info — 常规（默认）' },
  { value: 'debug', label: 'debug — 详细诊断' },
  { value: 'trace', label: 'trace — 极细' },
];

export function createMockSettingsPort(): SettingsPort {
  return {
    logLevelOptions: LOG_LEVEL_OPTIONS,

    async getSettings() {
      await delay(randomLatency(200, 300));
      state = loadState();
      saveString(StorageKey.theme, state.theme);
      applyTheme(state.theme);
      return { ...state };
    },

    async updateSettings(patch) {
      await delay(randomLatency(300, 300));
      state = { ...state, ...patch };
      saveJson(SETTINGS_KEY, state);
      if (patch.theme) {
        saveString(StorageKey.theme, patch.theme);
        applyTheme(patch.theme);
      }
      if (patch.language) {
        saveString(StorageKey.language, patch.language);
      }
      return { ...state };
    },

    async openLogsDir() {
      throw new Error('浏览器预览无法打开本地日志目录，请使用桌面版');
    },

    async openExternalUrl(url) {
      await delay(50);
      const opened = window.open(url, '_blank', 'noopener,noreferrer');
      if (!opened) {
        // Popup blocked — still resolve; user may allow and retry.
        throw new Error('浏览器拦截了弹窗，请允许本页打开新窗口后重试');
      }
    },

    async pickDirectory(options) {
      await delay(40);
      return promptForDirectoryPath(options?.title, options?.defaultPath);
    },

    async logGuiEvent() {
      // Browser mock has no GUI log file.
    },
  };
}

/** Browser cannot expose a real filesystem path; prompt is the mock stand-in. */
export function promptForDirectoryPath(
  title?: string,
  defaultPath?: string | null,
): string | null {
  if (typeof window === 'undefined' || typeof window.prompt !== 'function') {
    return null;
  }
  const entered = window.prompt(title ?? '选择工作目录（输入完整路径）', defaultPath ?? '');
  if (entered == null) return null;
  const trimmed = entered.trim();
  return trimmed || null;
}
