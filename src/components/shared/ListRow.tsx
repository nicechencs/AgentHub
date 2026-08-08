import type { HTMLAttributes, ReactNode } from 'react';
import { cn } from '@/lib/utils';

export type ListRowProps = HTMLAttributes<HTMLDivElement> & {
  /** 当前项（中性 bg-active；与 checkbox 多选无关） */
  active?: boolean;
  /**
   * 左侧指示条颜色（如 agent 品牌色）。
   * 未传时：active 用中性 border-strong 细条；非 active 无条。
   */
  indicatorColor?: string | null;
  /** 默认 true：active 时显示左边条 */
  showIndicator?: boolean;
  children: ReactNode;
};

/**
 * 列表行外壳：统一 active = bg-active + 可选左边条。
 * Connections / 侧栏式列表可复用；表格行请用 TableRow 的 active。
 */
export function ListRow({
  active = false,
  indicatorColor,
  showIndicator = true,
  className,
  children,
  style,
  ...props
}: ListRowProps) {
  const barColor =
    showIndicator && active
      ? indicatorColor?.trim() || 'var(--border-strong)'
      : null;

  return (
    <div
      data-active={active ? 'true' : undefined}
      className={cn(
        'relative rounded-card border border-border bg-panel transition-colors',
        !active && 'hover:bg-hover/50',
        active && 'border-border/80 bg-active',
        className,
      )}
      style={style}
      {...props}
    >
      {barColor ? (
        <span
          aria-hidden
          className="absolute inset-y-2 left-0 w-[3px] rounded-full"
          style={{ backgroundColor: barColor }}
        />
      ) : null}
      {children}
    </div>
  );
}
