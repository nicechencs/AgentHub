import * as React from 'react';
import * as TooltipPrimitive from '@radix-ui/react-tooltip';
import { cn } from '@/lib/utils';

const TooltipProvider = TooltipPrimitive.Provider;
const Tooltip = TooltipPrimitive.Root;
const TooltipTrigger = TooltipPrimitive.Trigger;

/**
 * 浮层提示：相对触发器定位（默认上方 + 较大偏移），避免浏览器原生 title
 * 贴在指针旁被鼠标遮挡。内容经 Portal 渲染，不被 overflow 裁切。
 *
 * 全局视觉唯一真源：所有悬停提示应走 Hint / Tip / Button·Input 的 title→Hint，
 * 或本组件；**禁止业务侧原生 `title=` 当教学/操作提示**（黄框 + 双通道）。
 * 默认延迟与 `main.tsx` TooltipProvider 一致（200ms）；仅可用性证明必要时才覆盖
 * `Hint.delayDuration`。
 */
const TooltipContent = React.forwardRef<
  React.ElementRef<typeof TooltipPrimitive.Content>,
  React.ComponentPropsWithoutRef<typeof TooltipPrimitive.Content>
>(({ className, side = 'top', sideOffset = 8, collisionPadding = 8, ...props }, ref) => (
  <TooltipPrimitive.Portal>
    <TooltipPrimitive.Content
      ref={ref}
      side={side}
      sideOffset={sideOffset}
      collisionPadding={collisionPadding}
      className={cn(
        'z-50 max-w-xs rounded-btn border border-border bg-panel px-2.5 py-1.5 text-left text-meta leading-snug text-primary shadow-sm',
        className,
      )}
      {...props}
    />
  </TooltipPrimitive.Portal>
));
TooltipContent.displayName = 'TooltipContent';

type HintSide = NonNullable<React.ComponentPropsWithoutRef<typeof TooltipContent>['side']>;

/**
 * 交互控件悬停提示：优先用此替代原生 `title`。
 * 对 disabled 子节点外包一层 span，保证仍可悬停显示。
 * 作为 DropdownMenuTrigger asChild 子节点时，把 Hint 包在 Trigger 外层。
 */
export function Hint({
  label,
  children,
  side = 'top',
  sideOffset = 8,
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
    <Tooltip delayDuration={delayDuration} disableHoverableContent>
      <TooltipTrigger asChild>{trigger}</TooltipTrigger>
      <TooltipContent side={side} sideOffset={sideOffset} className={contentClassName}>
        {label}
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
  sideOffset = 8,
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
