import { cn } from '@/lib/utils';

/**
 * 分段控件视觉真源（灰轨 + 白底抬起，无 accent 铺底）。
 *
 * ## 该统一
 * - 页内**列表筛选**（全部 / 已启用 / 类型…）→ `SegmentedControl` + **size=sm** + `count` 明文角标
 * - **Agent 切换条** → 全站只用 `AgentTabStrip`（md）；普通数字走 `counts`，不要页内自绘 badge
 * - 页级 **导航 Tabs**（Skills 三栏、设置分区）→ `Tabs`，项高与 md 一致
 * - 筛选/导航上的**普通数量** → `segmentedCountClass`（明文 muted）
 *
 * ## 不该统一（允许特例）
 * - 预览 chrome「预览 | 源码」：更扁 sm + 固定 h-6（工具条密度）
 * - Agent 条 **renderEnd** 特例：生效绿点、琥珀「可加入」等（不是普通 count；用 `actionCountClass`）
 * - Agent 品牌圆点：仅 AgentTabStrip
 */
export const segmentedTrackClass =
  'inline-flex flex-wrap items-center gap-0.5 rounded-card bg-hover p-0.5';

/** 筛选/导航旁的普通数量角标（连接页同源） */
export const segmentedCountClass = 'tabular-nums text-muted';

/**
 * 琥珀行动角标（非普通 count）：如 Skills「只在本工具 / 可加入共享库」。
 * 用 design token，避免页内 amber-* 漂移。
 */
export const actionCountClass =
  'rounded-full bg-warning/15 px-1.5 py-0 text-2xs tabular-nums text-warning';

export type SegmentedSize = 'sm' | 'md';

/** 与 sm / md 档位对应的内边距与字号 */
export function segmentedItemSizeClass(size: SegmentedSize = 'md'): string {
  return size === 'sm' ? 'px-2.5 py-1 text-xs' : 'px-2.5 py-1.5 text-sm';
}

export function segmentedItemClass(
  active: boolean,
  size: SegmentedSize = 'md',
  options?: { disabled?: boolean },
): string {
  return cn(
    'inline-flex items-center justify-center rounded-btn transition-colors',
    segmentedItemSizeClass(size),
    active
      ? 'bg-panel font-medium text-primary shadow-sm'
      : 'text-secondary hover:bg-panel/50 hover:text-primary',
    options?.disabled && 'cursor-not-allowed opacity-40',
  );
}
