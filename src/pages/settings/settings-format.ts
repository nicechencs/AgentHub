import type { TranslateFn } from '@/lib/i18n';
import type { AppSettings, LogLevel, SkillMarketSource } from '@/lib/types';

export const GITHUB_REPO_URL = 'https://github.com/nicechencs/AgentHub';

export const SKILL_MARKET_VALUES: SkillMarketSource[] = ['auto', 'skills.sh', 'skillhub.cn'];

export const LOG_LEVEL_VALUES: LogLevel[] = ['error', 'warn', 'info', 'debug', 'trace'];

/** Canonical Settings `?tab=` slugs. */
export const SETTINGS_TABS = ['preferences', 'local', 'about'] as const;
export type SettingsTab = (typeof SETTINGS_TABS)[number];

/** In-page focus after a tab is resolved (hash `#backups`). */
export type SettingsSection = 'backups';

export const SETTINGS_BACKUPS_HASH = 'backups';

/**
 * Legacy `?tab=` slugs → canonical tab (+ optional hash).
 * `general` → Preferences; `security` → About (credential note);
 * `data` → Local (top); `backups` → Local `#backups`.
 */
export const SETTINGS_TAB_REDIRECTS: Record<string, { tab: SettingsTab; hash?: string }> = {
  preferences: { tab: 'preferences' },
  local: { tab: 'local' },
  about: { tab: 'about' },
  general: { tab: 'preferences' },
  security: { tab: 'about' },
  data: { tab: 'local' },
  backups: { tab: 'local', hash: SETTINGS_BACKUPS_HASH },
};

export function isSettingsTab(raw: string | null | undefined): raw is SettingsTab {
  return !!raw && (SETTINGS_TABS as readonly string[]).includes(raw);
}

/** Resolve any `?tab=` value to a canonical tab. Unknown / empty → Preferences. */
export function parseSettingsTab(raw: string | null): SettingsTab {
  if (!raw) return 'preferences';
  const mapped = SETTINGS_TAB_REDIRECTS[raw];
  if (mapped) return mapped.tab;
  return 'preferences';
}

export function parseSettingsHash(hash: string | null | undefined): SettingsSection | null {
  const value = (hash ?? '').replace(/^#/, '');
  return value === SETTINGS_BACKUPS_HASH ? 'backups' : null;
}

export interface SettingsLocation {
  tab: SettingsTab;
  hash: string;
  /** True when the incoming `?tab=` is legacy, unknown, or needs a backups hash. */
  shouldReplace: boolean;
}

/**
 * Central URL resolver: canonical tab + optional `#backups`.
 * Old slugs and illegal values are marked for replace-navigation.
 */
export function resolveSettingsLocation(
  rawTab: string | null,
  hash: string | null | undefined,
): SettingsLocation {
  const mapped = rawTab ? SETTINGS_TAB_REDIRECTS[rawTab] : undefined;
  const tab = mapped?.tab ?? 'preferences';
  const fromLegacyBackups = rawTab === 'backups';
  const existingHash = parseSettingsHash(hash);
  const nextHash =
    fromLegacyBackups || existingHash === 'backups' ? SETTINGS_BACKUPS_HASH : '';

  const canonicalTab = isSettingsTab(rawTab);
  const hashOk = (nextHash === '' && !existingHash) || (nextHash === SETTINGS_BACKUPS_HASH && existingHash === 'backups');
  const shouldReplace = rawTab !== null && (!canonicalTab || (fromLegacyBackups && !hashOk));

  return { tab, hash: nextHash, shouldReplace };
}

export function settingsSearch(tab: SettingsTab): string {
  return `?tab=${tab}`;
}

export function skillMarketLabel(
  source: SkillMarketSource | undefined,
  t: TranslateFn,
): string {
  const value = source ?? 'auto';
  if (value === 'skills.sh') return t('settings.general.skillMarketSkillsSh');
  if (value === 'skillhub.cn') return t('settings.general.skillMarketSkillhub');
  return t('settings.general.skillMarketAuto');
}

export function logLevelOptionLabel(level: LogLevel, t: TranslateFn): string {
  switch (level) {
    case 'error':
      return t('settings.data.logLevel_error');
    case 'warn':
      return t('settings.data.logLevel_warn');
    case 'info':
      return t('settings.data.logLevel_info');
    case 'debug':
      return t('settings.data.logLevel_debug');
    case 'trace':
      return t('settings.data.logLevel_trace');
  }
}

export function clampLogRetentionDays(n: number): number {
  return Math.min(365, Math.max(1, n));
}

export function clampUsageIntervalMin(n: number): number {
  return Math.min(24 * 60, Math.max(0, n));
}

export function fmtRelativeI18n(iso: string | undefined, t: TranslateFn): string {
  if (!iso) return '—';
  const diff = Date.now() - new Date(iso).getTime();
  const m = Math.floor(diff / 60000);
  if (m < 1) return t('common.relativeJustNow');
  if (m < 60) return t('common.relativeMinutes', { n: m });
  const h = Math.floor(m / 60);
  if (h < 24) return t('common.relativeHours', { n: h });
  return t('common.relativeDays', { n: Math.floor(h / 24) });
}

export function fmtAbsoluteI18n(iso: string, lang: AppSettings['language']): string {
  return new Date(iso).toLocaleString(lang === 'en' ? 'en-US' : 'zh-CN', { hour12: false });
}
