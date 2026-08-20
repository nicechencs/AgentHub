import type { ReactNode } from 'react';
import { Tip } from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';

/**
 * 页头：标题短、描述短。需要补充说明时用 descriptionTip（悬停展示），
 * 勿把长说明直接铺在页面上。
 *
 * - `default`：常规页（Dashboard / Settings…）
 * - `compact`：全高页（Skills / Projects / Chat 壳内）— 同档标题，只收底边距
 */
export function PageHeader({
  title,
  badge,
  description,
  descriptionTip,
  actions,
  size = 'default',
  className,
}: {
  title: string;
  /** 标题旁状态标记（如更新 pin / 运行状态） */
  badge?: ReactNode;
  description?: string;
  /** 悬停时的详细说明；有则 description 可更短 */
  descriptionTip?: string;
  actions?: ReactNode;
  size?: 'default' | 'compact';
  className?: string;
}) {
  const compact = size === 'compact';

  return (
    <div
      className={cn(
        'flex items-start justify-between gap-4',
        compact ? 'mb-2' : 'mb-4',
        className,
      )}
    >
      <div className="min-w-0">
        <div className="flex min-w-0 items-center gap-2">
          <h1 className="text-title font-semibold tracking-tight text-primary">
            {title}
          </h1>
          {badge}
        </div>
        {description &&
          (descriptionTip ? (
            <Tip
              className="mt-0.5 block text-meta text-secondary"
              label={descriptionTip}
            >
              {description}
            </Tip>
          ) : (
            <p className="mt-0.5 text-meta text-secondary">
              {description}
            </p>
          ))}
      </div>
      {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
    </div>
  );
}
