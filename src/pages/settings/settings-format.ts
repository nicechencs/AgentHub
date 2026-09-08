import type { MessageKey, TranslateFn } from '@/lib/i18n';
import type { OptionalNavId } from '@/lib/ui-preferences';
import type { LogLevel, SkillMarketSource } from '@/lib/types';

export { GITHUB_NEW_ISSUE_URL, GITHUB_REPO_URL } from '@/lib/github';

export const SKILL_MARKET_VALUES: SkillMarketSource[] = ['auto', 'skills.sh', 'skillhub.cn'];

export const LOG_LEVEL_VALUES: LogLevel[] = ['error', 'warn', 'info', 'debug', 'trace'];

/** Canonical Settings `?tab=` slugs. */
export const SETTINGS_TABS = ['preferences', 'features', 'local', 'backups', 'about'] as const;
export type SettingsTab = (typeof SETTINGS_TABS)[number];

/**
 * Legacy `?tab=` slugs → canonical tab.
 * `general` → Preferences; `security` → About (login information note);
 * `data` → Local. `backups` is a canonical tab. Legacy `#backups` /
 * `?tab=local#backups` → Backups (see {@link resolveSettingsLocation}).
 */
export const SETTINGS_TAB_REDIRECTS: Record<string, { tab: SettingsTab }> = {
  preferences: { tab: 'preferences' },
  features: { tab: 'features' },
  local: { tab: 'local' },
  backups: { tab: 'backups' },
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

export interface SettingsLocation {
  tab: SettingsTab;
  /** Fragment without `#`. Canonical backups deep-link is `backups`. */
  hash: string;
  /** True when the incoming `?tab=` / hash is legacy or unknown. */
  shouldReplace: boolean;
}

/**
 * Central URL resolver: canonical tab + hash.
 * Old slugs, a standalone `backups` slug, `#backups` hashes, and illegal
 * values are marked for replace-navigation.
 */
export function resolveSettingsLocation(
  rawTab: string | null,
  hash?: string | null,
): SettingsLocation {
  const hashValue = (hash ?? '').replace(/^#/, '');
  const wantsBackups = hashValue === 'backups' || rawTab === 'backups';
  if (wantsBackups) {
    const alreadyCanonical = rawTab === 'backups' && hashValue === '';
    return { tab: 'backups', hash: '', shouldReplace: !alreadyCanonical };
  }
  const mapped = rawTab ? SETTINGS_TAB_REDIRECTS[rawTab] : undefined;
  const tab = mapped?.tab ?? 'preferences';
  const shouldReplace = rawTab !== null && !isSettingsTab(rawTab);
  return { tab, hash: '', shouldReplace };
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

/** 设置 → 侧栏：每个可开关入口的文案。顺序由 OPTIONAL_NAV_IDS 决定。 */
export const OPTIONAL_NAV_VISIBLE_COPY: Record<
  OptionalNavId,
  { label: MessageKey; description: MessageKey; tip: MessageKey }
> = {
  skills: {
    label: 'settings.general.skillsNavVisibleLabel',
    description: 'settings.general.skillsNavVisibleDescription',
    tip: 'settings.general.skillsNavVisibleTip',
  },
  mcp: {
    label: 'settings.general.mcpNavVisibleLabel',
    description: 'settings.general.mcpNavVisibleDescription',
    tip: 'settings.general.mcpNavVisibleTip',
  },
  projects: {
    label: 'settings.general.projectsNavVisibleLabel',
    description: 'settings.general.projectsNavVisibleDescription',
    tip: 'settings.general.projectsNavVisibleTip',
  },
  plugins: {
    label: 'settings.general.pluginsNavVisibleLabel',
    description: 'settings.general.pluginsNavVisibleDescription',
    tip: 'settings.general.pluginsNavVisibleTip',
  },
  connections: {
    label: 'settings.general.connectionsNavVisibleLabel',
    description: 'settings.general.connectionsNavVisibleDescription',
    tip: 'settings.general.connectionsNavVisibleTip',
  },
  sub2api: {
    label: 'settings.general.sub2apiNavVisibleLabel',
    description: 'settings.general.sub2apiNavVisibleDescription',
    tip: 'settings.general.sub2apiNavVisibleTip',
  },
  routes: {
    label: 'settings.general.routesNavVisibleLabel',
    description: 'settings.general.routesNavVisibleDescription',
    tip: 'settings.general.routesNavVisibleTip',
  },
};
