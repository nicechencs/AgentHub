import type { AppSettings, SkillMarketSource } from '@/lib/types';

export const GITHUB_REPO_URL = 'https://github.com/nicechencs/AgentHub';

export const SKILL_MARKET_OPTIONS: { value: SkillMarketSource; label: string }[] = [
  { value: 'auto', label: '自动（不通则切换）' },
  { value: 'skills.sh', label: 'skills.sh' },
  { value: 'skillhub.cn', label: 'skillhub.cn' },
];

export const SETTINGS_TABS = ['general', 'security', 'data', 'backups', 'about'] as const;
export type SettingsTab = (typeof SETTINGS_TABS)[number];

export function parseSettingsTab(raw: string | null): SettingsTab {
  if (raw && (SETTINGS_TABS as readonly string[]).includes(raw)) {
    return raw as SettingsTab;
  }
  return 'general';
}

export function skillMarketLabel(source: SkillMarketSource | undefined): string {
  const value = source ?? 'auto';
  return SKILL_MARKET_OPTIONS.find((opt) => opt.value === value)?.label ?? '自动（不通则切换）';
}

/** 常规区保存成功摘要：覆盖本区实际写入项，避免只提技能市场。 */
export function generalSettingsSaveDescription(s: AppSettings): string {
  const tray = s.closeToTray ? '关闭时到托盘' : '关闭时直接退出';
  return `${tray} · 技能市场 ${skillMarketLabel(s.skillMarketSource)}`;
}

/** 数据区保存成功摘要：区分立即生效与需重启项。 */
export function dataSettingsSaveDescription(s: AppSettings): string {
  const usage =
    s.usageCollectIntervalMin <= 0
      ? '用量仅手动采集'
      : `用量每 ${s.usageCollectIntervalMin} 分钟自动采集`;
  return `${usage}；日志级别 ${s.logLevel} 下次启动生效（保留 ${s.logRetentionDays} 天）`;
}
