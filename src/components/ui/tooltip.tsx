import * as React from 'react';
import * as TooltipPrimitive from '@radix-ui/react-tooltip';
import { TOOLTIP } from '@/styles/tokens';
import { cn } from '@/lib/utils';

const TooltipProvider = TooltipPrimitive.Provider;
const Tooltip = TooltipPrimitive.Root;
const TooltipTrigger = TooltipPrimitive.Trigger;

/** 气泡铬层锁在此类；`contentClassName` 只排泡内，改不了宽高/底色/圆角。 */
export const TOOLTIP_SURFACE_CLASS = [
  'z-50 box-border w-max',
  'max-w-[var(--tooltip-max-width)]',
  'max-h-[min(var(--tooltip-max-height),calc(100vh-16px))]',
  'overflow-x-hidden overflow-y-auto',
  'break-words [overflow-wrap:anywhere]',
  'rounded-card border border-border bg-panel shadow-sm',
  'px-[var(--tooltip-pad-x)] py-[var(--tooltip-pad-y)]',
  'text-left font-sans text-meta font-normal leading-[var(--font-meta-leading)] text-primary',
].join(' ');

/** Recharts 等不能写 Tailwind class 的浮层，与 Hint 气泡同一套表面。 */
export function tooltipSurfaceStyle(): React.CSSProperties {
  return {
    backgroundColor: 'var(--bg-panel)',
    border: '1px solid var(--border)',
    borderRadius: 'var(--radius)',
    boxShadow: 'var(--shadow-sm)',
    color: 'var(--text-primary)',
    fontFamily:
      'system-ui, -apple-system, "Segoe UI", Roboto, "PingFang SC", "Microsoft YaHei", sans-serif',
    fontSize: 'var(--font-meta-size)',
    fontWeight: 400,
    lineHeight: 'var(--font-meta-leading)',
    padding: `${TOOLTIP.paddingY} ${TOOLTIP.paddingX}`,
    maxWidth: TOOLTIP.maxWidth,
    maxHeight: `min(${TOOLTIP.maxHeight}, calc(100vh - 16px))`,
    overflowX: 'hidden',
    overflowY: 'auto',
    overflowWrap: 'anywhere',
    wordBreak: 'break-word',
    whiteSpace: 'normal',
  };
}

const TooltipContent = React.forwardRef<
  React.ElementRef<typeof TooltipPrimitive.Content>,
  React.ComponentPropsWithoutRef<typeof TooltipPrimitive.Content>
>(
  (
    {
      className,
      side = 'top',
      sideOffset = TOOLTIP.sideOffset,
      collisionPadding = TOOLTIP.collisionPadding,
      ...props
    },
    ref,
  ) => (
    <TooltipPrimitive.Portal>
      <TooltipPrimitive.Content
        ref={ref}
        side={side}
        sideOffset={sideOffset}
        collisionPadding={collisionPadding}
        avoidCollisions
        className={cn(className, TOOLTIP_SURFACE_CLASS)}
        {...props}
      />
    </TooltipPrimitive.Portal>
  ),
);
TooltipContent.displayName = 'TooltipContent';

type HintSide = NonNullable<React.ComponentPropsWithoutRef<typeof TooltipContent>['side']>;

function TooltipBody({
  contentClassName,
  children,
}: {
  contentClassName?: string;
  children: React.ReactNode;
}) {
  if (!contentClassName) return children;
  return <div className={cn('min-w-0', contentClassName)}>{children}</div>;
}

/**
 * 交互控件悬停提示：优先用此替代原生 `title`。
 * 对 disabled 子节点外包一层 span，保证仍可悬停显示。
 * 作为 DropdownMenuTrigger asChild 子节点时，把 Hint 包在 Trigger 外层。
 */
export function Hint({
  label,
  children,
  side = 'top',
  sideOffset = TOOLTIP.sideOffset,
  contentClassName,
  delayDuration,
}: {
  label?: React.ReactNode;
  children: React.ReactElement;
  side?: HintSide;
  sideOffset?: number;
  contentClassName?: string;
  delayDuration?: number;
}) {
  if (label == null || label === false || label === '') {
    return children;
  }

  const child = children as React.ReactElement<{ disabled?: boolean }>;
  const isDisabled = Boolean(child.props.disabled);
  const trigger = isDisabled ? (
    <span className="inline-flex max-w-full" tabIndex={0}>
      {children}
    </span>
  ) : (
    children
  );

  return (
    <Tooltip delayDuration={delayDuration}>
      <TooltipTrigger asChild>{trigger}</TooltipTrigger>
      <TooltipContent side={side} sideOffset={sideOffset}>
        <TooltipBody contentClassName={contentClassName}>{label}</TooltipBody>
      </TooltipContent>
    </Tooltip>
  );
}

/**
 * 文本/截断路径等非控件节点的悬停提示。
 * 内部包一层 span 再走 Hint，避免浏览器原生 title 黄框。
 */
export function Tip({
  label,
  children,
  className,
  side = 'top',
  sideOffset = TOOLTIP.sideOffset,
  contentClassName,
}: {
  label?: React.ReactNode;
  children: React.ReactNode;
  className?: string;
  side?: HintSide;
  sideOffset?: number;
  contentClassName?: string;
}) {
  if (label == null || label === false || label === '') {
    return <span className={className}>{children}</span>;
  }
  return (
    <Hint
      label={label}
      side={side}
      sideOffset={sideOffset}
      contentClassName={contentClassName}
    >
      <span className={cn('inline-block max-w-full min-w-0', className)}>{children}</span>
    </Hint>
  );
}

export { Tooltip, TooltipTrigger, TooltipContent, TooltipProvider };
export type { HintSide };
export { TOOLTIP };
