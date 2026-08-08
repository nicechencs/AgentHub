import * as React from 'react';
import { cn } from '@/lib/utils';

export type CardVariant = 'default' | 'plain' | 'subtle';

/**
 * 表面卡片。
 * - `default`：panel + 边框 + 轻阴影 xs（独立内容块）
 * - `plain`：仅 panel 底，无边框阴影（嵌在已有框内或工具条，避免双重描边）
 * - `subtle`：弱底无边框（条带/批处理条）
 *
 * 内边距约定（间距阶梯 4/8/12/16…）：
 * - 组合件 CardHeader / Content / Footer：水平 16（px-4）
 * - 自管 padding 的列表/指标卡：优先 `p-3`（12）紧凑 或 `p-4`（16）常规
 */
export function Card({
  className,
  variant = 'default',
  ...props
}: React.HTMLAttributes<HTMLDivElement> & { variant?: CardVariant }) {
  return (
    <div
      className={cn(
        'rounded-card',
        variant === 'default' && 'border border-border bg-panel shadow-xs',
        variant === 'plain' && 'border-0 bg-panel shadow-none',
        variant === 'subtle' && 'border-0 bg-subtle shadow-none',
        className,
      )}
      data-card={variant}
      {...props}
    />
  );
}

export function CardHeader({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div className={cn('flex items-center justify-between px-4 py-3', className)} {...props} />
  );
}

export function CardTitle({ className, ...props }: React.HTMLAttributes<HTMLHeadingElement>) {
  // 区块标题 14px（text-base）
  return <h3 className={cn('text-base font-medium', className)} {...props} />;
}

export function CardContent({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return <div className={cn('px-4 pb-4', className)} {...props} />;
}

export function CardFooter({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn(
        'flex items-center justify-end gap-2 border-t border-border px-4 py-3',
        className,
      )}
      {...props}
    />
  );
}
