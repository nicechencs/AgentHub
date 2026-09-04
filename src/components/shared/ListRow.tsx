import type { HTMLAttributes, KeyboardEvent, MouseEvent, ReactNode } from 'react';
import { cn } from '@/lib/utils';

/** Shared padding for Agent / 连接 list cards. */
export const LIST_ROW_PAD = 'p-3';

function isInteractiveListTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  return Boolean(
    target.closest('button, a, input, textarea, [role="button"], [role="menuitem"]'),
  );
}

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
  /** Click empty area of the row to open details; ignores buttons/links. */
  onOpen?: () => void;
  children: ReactNode;
};

/**
 * 列表行外壳：统一 active = bg-active + 可选左边条。
 * Connections / 侧栏式列表可复用；表格行请用 TableRow 的 active / onOpen。
 */
export function ListRow({
  active = false,
  indicatorColor,
  showIndicator = true,
  onOpen,
  className,
  children,
  style,
  onClick,
  onKeyDown,
  tabIndex,
  ...props
}: ListRowProps) {
  const barColor =
    showIndicator && active
      ? indicatorColor?.trim() || 'var(--border-strong)'
      : null;

  const handleClick = (event: MouseEvent<HTMLDivElement>) => {
    onClick?.(event);
    if (event.defaultPrevented || !onOpen) return;
    if (isInteractiveListTarget(event.target)) return;
    onOpen();
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    onKeyDown?.(event);
    if (event.defaultPrevented || !onOpen) return;
    if (event.key !== 'Enter' && event.key !== ' ') return;
    if (isInteractiveListTarget(event.target)) return;
    event.preventDefault();
    onOpen();
  };

  return (
    <div
      {...props}
      data-active={active ? 'true' : undefined}
      tabIndex={onOpen ? (tabIndex ?? 0) : tabIndex}
      className={cn(
        'relative rounded-card border border-border bg-panel transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60',
        !active && 'hover:bg-hover/50',
        active && 'border-border-strong bg-active',
        onOpen && 'cursor-pointer',
        className,
      )}
      style={style}
      onClick={handleClick}
      onKeyDown={handleKeyDown}
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

/**
 * Inner layout for workbench list cards (Agents / 连接):
 * [leading] [title + meta, wrapping] [actions].
 * Vertically centers logo/text/buttons on one row.
 */
export function ListRowBody({
  leading,
  main,
  actions,
}: {
  leading?: ReactNode;
  main: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
      {leading}
      <div className="flex min-w-0 flex-1 flex-wrap items-center gap-x-2 gap-y-1">
        {main}
      </div>
      {actions ? (
        <div className="flex shrink-0 items-center gap-2">{actions}</div>
      ) : null}
    </div>
  );
}
