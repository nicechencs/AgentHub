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
}) {
  const resolved =
    color ??
    (agentId ? AGENT_MAP[agentId]?.color : undefined) ??
    'var(--text-muted)';
  const label =
    title === null ? undefined : (title ?? (agentId ? AGENT_MAP[agentId]?.name : undefined));

  const dot = (
    <span
      className={cn(
        'inline-block shrink-0 rounded-full',
        SIZE[size],
        ring && 'ring-1 ring-panel',
        className,
      )}
      style={{ backgroundColor: resolved }}
      aria-hidden={label ? undefined : true}
    />
  );

  if (!label) return dot;
  return <Hint label={label}>{dot}</Hint>;
}
