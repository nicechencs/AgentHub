import type { ReactNode } from 'react';
import { Tip } from '@/components/ui/tooltip';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { cn } from '@/lib/utils';

/**
 * 页头：标题短、描述短。需要补充说明时用 descriptionTip（悬停展示），
 * 勿把长说明直接铺在页面上。
 *
 * 标题槽固定两行（`pageTitle` + 一行 meta），切页字号/高度/左缘对齐。
 * - `default`：常规页；底距 18px（顶/左右由 pageShell 提供）
 * - `compact`：全高页；底距由 `workbenchHeader` 的 18px 提供，自身不再加 mb
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
        compact ? 'mb-0' : 'mb-[18px]',
        className,
      )}
    >
      <div className={cn('min-w-0', pageRhythm.pageTitleBlock)}>
        <div className="flex min-w-0 items-center gap-2">
          <h1 className={pageRhythm.pageTitle}>
            {title}
          </h1>
          {badge}
        </div>
        {description && descriptionTip ? (
          <Tip
            className="mt-0.5 block truncate text-meta text-secondary"
            label={descriptionTip}
          >
            {description}
          </Tip>
        ) : (
          <p className="mt-0.5 truncate text-meta text-secondary">
            {description || '\u00a0'}
          </p>
        )}
      </div>
      {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
    </div>
  );
}
