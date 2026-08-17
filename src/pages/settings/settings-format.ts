import type { TranslateFn } from '@/lib/i18n';
import type { AppSettings, LogLevel, SkillMarketSource } from '@/lib/types';

export const GITHUB_REPO_URL = 'https://github.com/nicechencs/AgentHub';

export const SKILL_MARKET_VALUES: SkillMarketSource[] = ['auto', 'skills.sh', 'skillhub.cn'];

export const LOG_LEVEL_VALUES: LogLevel[] = ['error', 'warn', 'info', 'debug', 'trace'];

export const SETTINGS_TABS = ['general', 'security', 'data', 'backups', 'about'] as const;
export type SettingsTab = (typeof SETTINGS_TABS)[number];

export function parseSettingsTab(raw: string | null): SettingsTab {
  if (raw && (SETTINGS_TABS as readonly string[]).includes(raw)) {
    return raw as SettingsTab;
  }
  return 'general';
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

/** 常规区保存成功摘要：覆盖本区实际写入项，避免只提技能市场。 */
export function generalSettingsSaveDescription(s: AppSettings, t: TranslateFn): string {
  const tray = s.closeToTray ? t('settings.general.trayOnClose') : t('settings.general.quitOnClose');
  return `${tray} · ${t('settings.general.skillMarketSummary', {
    label: skillMarketLabel(s.skillMarketSource, t),
  })}`;
}

/** 数据区保存成功摘要：区分立即生效与需重启项。 */
export function dataSettingsSaveDescription(s: AppSettings, t: TranslateFn): string {
  const usage =
    s.usageCollectIntervalMin <= 0
      ? t('settings.data.usageManual')
      : t('settings.data.usageAuto', { minutes: s.usageCollectIntervalMin });
  return `${usage}；${t('settings.data.logLevelRestart', {
    level: s.logLevel,
    days: s.logRetentionDays,
  })}`;
}

export function generalSettingsPayload(s: AppSettings): Partial<AppSettings> {
  return {
    theme: s.theme,
    language: s.language,
    autoStart: s.autoStart,
    closeToTray: s.closeToTray,
    skillMarketSource: s.skillMarketSource ?? 'auto',
  };
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
