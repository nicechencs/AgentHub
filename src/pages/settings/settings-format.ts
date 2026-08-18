import type { TranslateFn } from '@/lib/i18n';
import type { LogLevel, SkillMarketSource } from '@/lib/types';

export const GITHUB_REPO_URL = 'https://github.com/nicechencs/AgentHub';

export const SKILL_MARKET_VALUES: SkillMarketSource[] = ['auto', 'skills.sh', 'skillhub.cn'];

export const LOG_LEVEL_VALUES: LogLevel[] = ['error', 'warn', 'info', 'debug', 'trace'];

/** Canonical Settings `?tab=` slugs. */
export const SETTINGS_TABS = ['preferences', 'local', 'about'] as const;
export type SettingsTab = (typeof SETTINGS_TABS)[number];

/**
 * Legacy `?tab=` slugs → canonical tab.
 * `general` → Preferences; `security` → About (credential note);
 * `data` → Local. `backups` is not a Settings tab — see {@link settingsBackupsRedirect}.
 */
export const SETTINGS_TAB_REDIRECTS: Record<string, { tab: SettingsTab }> = {
  preferences: { tab: 'preferences' },
  local: { tab: 'local' },
  about: { tab: 'about' },
  general: { tab: 'preferences' },
  security: { tab: 'about' },
  data: { tab: 'local' },
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

/** Old Settings backups deep links → standalone `/backups`. */
export function settingsBackupsRedirect(
  rawTab: string | null,
  hash?: string | null,
): '/backups' | null {
  if (rawTab === 'backups') return '/backups';
  const value = (hash ?? '').replace(/^#/, '');
  return value === 'backups' ? '/backups' : null;
}

export interface SettingsLocation {
  tab: SettingsTab;
  /** True when the incoming `?tab=` is legacy or unknown. */
  shouldReplace: boolean;
}

/**
 * Central URL resolver: canonical tab.
 * Old slugs and illegal values are marked for replace-navigation.
 */
export function resolveSettingsLocation(rawTab: string | null): SettingsLocation {
  const mapped = rawTab ? SETTINGS_TAB_REDIRECTS[rawTab] : undefined;
  const tab = mapped?.tab ?? 'preferences';
  const shouldReplace = rawTab !== null && !isSettingsTab(rawTab);
  return { tab, shouldReplace };
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
