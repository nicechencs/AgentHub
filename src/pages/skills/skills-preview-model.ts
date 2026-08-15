import { pageEdgePx } from '@/components/layout/page-rhythm';
import type { SkillMarketSource } from '@/lib/types';

export const PREVIEW_WIDTH_DEFAULT = 440;
/** 正常拖拽/记忆宽度下限 */
export const PREVIEW_WIDTH_MIN = 300;
/** 视口极窄时允许压到的硬底（仍可横滑看文档） */
export const PREVIEW_WIDTH_FLOOR = 240;
/** 预览打开时给左侧列表预留的舒适宽度 */
export const MAIN_WIDTH_MIN = 380;
/** 极窄时左侧可再让一点，避免预览被裁到消失 */
export const MAIN_WIDTH_FLOOR = 280;
/** 预览卡片与窗边：水平 24 与 pageShell/workbenchX 一致 */
export const PREVIEW_FRAME_PAD_RIGHT = pageEdgePx.x;
export const PREVIEW_FRAME_PAD_Y = pageEdgePx.previewY;
export const PREVIEW_SEPARATOR_W = pageEdgePx.separator;
export const PREVIEW_WIDTH_STORAGE_KEY = 'agenthub.skills.previewWidth';
export const PREVIEW_WIDTH_STEP = 16;
export const PREVIEW_WIDTH_STEP_LARGE = 48;

export function readStoredPreviewWidth(): number {
  if (typeof window === 'undefined') return PREVIEW_WIDTH_DEFAULT;
  try {
    const raw = window.localStorage.getItem(PREVIEW_WIDTH_STORAGE_KEY);
    const n = raw ? Number(raw) : NaN;
    if (Number.isFinite(n) && n >= PREVIEW_WIDTH_MIN) return Math.round(n);
  } catch {
    // ignore
  }
  return PREVIEW_WIDTH_DEFAULT;
}

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

/** library=本地表 · market=市场；workspace / installed 兼容旧 URL */
export const SKILL_TABS = ['library', 'market'] as const;
export type SkillTab = (typeof SKILL_TABS)[number];

export function parseSkillTab(raw: string | null): SkillTab {
  if (raw === 'installed' || raw === 'workspace') return 'library';
  if (raw && (SKILL_TABS as readonly string[]).includes(raw)) return raw as SkillTab;
  return 'library';
}

export type LocalFilter = 'all' | 'private' | 'mapped' | 'unmapped' | 'conflict';
