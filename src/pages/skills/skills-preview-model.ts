import type { SkillMarketSource } from '@/lib/types';

export function marketSourceLabel(source: SkillMarketSource): string {
  if (source === 'skillhub.cn') return 'skillhub.cn';
  if (source === 'skills.sh') return 'skills.sh';
  return '自动';
}

export function marketHomeUrl(activeProvider: string | undefined, source: SkillMarketSource): string {
  if (activeProvider === 'skillhub.cn' || source === 'skillhub.cn') return 'https://skillhub.cn/';
  return 'https://skills.sh/';
}

/** 结果页展示的当前源名：优先实际 provider，否则用设置项 */
export function marketResultLabel(
  activeProvider: string | undefined,
  source: SkillMarketSource,
): string {
  if (activeProvider === 'skills.sh' || activeProvider === 'skillhub.cn') {
    return activeProvider;
  }
  return marketSourceLabel(source);
}

/** library=用户技能 · project=项目技能 · market=市场；workspace / installed 兼容旧 URL */
export const SKILL_TABS = ['library', 'project', 'market'] as const;
export type SkillTab = (typeof SKILL_TABS)[number];

export function parseSkillTab(raw: string | null): SkillTab {
  if (raw === 'installed' || raw === 'workspace') return 'library';
  if (raw && (SKILL_TABS as readonly string[]).includes(raw)) return raw as SkillTab;
  return 'library';
}

export type LocalFilter = 'all' | 'private' | 'mapped' | 'unmapped' | 'conflict';
