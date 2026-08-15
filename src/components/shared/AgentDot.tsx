import type * as React from 'react';
import { AGENT_MAP } from '@/config/agents';
import type { AgentId } from '@/lib/types';
import { Hint } from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';

const SIZE = {
  sm: 'h-1.5 w-1.5',
  md: 'h-2 w-2',
  lg: 'h-2.5 w-2.5',
} as const;

/**
 * 侧栏状态条等场景：hover 放大倍数（与 HOVER_GROW_CLASS 同源，改这里即可）。
 * 用 CSS 变量驱动 scale，避免业务侧散落任意 Tailwind 字面量。
 */
export const AGENT_DOT_HOVER_SCALE = 2.5;

const HOVER_GROW_CLASS =
  'origin-center transition-transform duration-150 ease-out hover:scale-[var(--agent-dot-hover-scale)]';

/**
 * Agent 品牌色圆点（唯一出口）。
 * 业务侧禁止再写 style={{ backgroundColor: meta.color }} + h-2 w-2 rounded-full。
 *
 * title:
 * - 省略 → 用 agent 名称
 * - string → 自定义提示
 * - null → 不显示提示（外层已有 Hint 时用）
 */
export function AgentDot({
  agentId,
  color,
  size = 'md',
  className,
  title,
  ring,
  growOnHover = false,
  hoverScale = AGENT_DOT_HOVER_SCALE,
  style: styleProp,
}: {
  /** 优先用 agentId 从 AGENTS 取色 */
  agentId?: AgentId;
  /** 无 agentId 时可用原始 CSS 变量 / 色值（如 meta.color） */
  color?: string;
  size?: keyof typeof SIZE;
  className?: string;
  title?: string | null;
  /** 侧栏等需要与面板底区分时加 ring */
  ring?: boolean;
  /** 悬停放大（状态条等小圆点可扫读） */
  growOnHover?: boolean;
  /** 覆盖默认 {@link AGENT_DOT_HOVER_SCALE}；仅 growOnHover 时生效 */
  hoverScale?: number;
  /** Extra inline styles (e.g. animationDelay on BootSplash). backgroundColor is owned by AgentDot. */
  style?: React.CSSProperties;
}) {
  const resolved =
    color ??
    (agentId ? AGENT_MAP[agentId]?.color : undefined) ??
    'var(--text-muted)';
  const label =
    title === null ? undefined : (title ?? (agentId ? AGENT_MAP[agentId]?.name : undefined));

  const style: React.CSSProperties = {
    ...styleProp,
    backgroundColor: resolved,
    ...(growOnHover
      ? ({ ['--agent-dot-hover-scale']: String(hoverScale) } as React.CSSProperties)
      : null),
  };

  const dot = (
    <span
      className={cn(
        'inline-block shrink-0 rounded-full',
        SIZE[size],
        ring && 'ring-1 ring-panel',
        growOnHover && HOVER_GROW_CLASS,
        className,
      )}
      style={style}
      aria-hidden={label ? undefined : true}
    />
  );

  if (!label) return dot;
  return <Hint label={label}>{dot}</Hint>;
}
