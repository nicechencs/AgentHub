import type { ReactNode } from 'react';
import { useRegisterPageChrome } from '@/components/layout/PageChromeContext';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { Tip } from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';

/**
 * 顶栏标题槽：同一行里标题（大、深）+ 说明（小、浅），切页字号/高度/左缘对齐。
 */
export function PageTitleBlock({
  title,
  badge,
  description,
  descriptionTip,
}: {
  title: string;
  badge?: ReactNode;
  description?: string;
  descriptionTip?: string;
}) {
  const meta = description ? (
    descriptionTip ? (
      <Tip className={pageRhythm.pageTitleMeta} label={descriptionTip}>
        {description}
      </Tip>
    ) : (
      <span className={pageRhythm.pageTitleMeta}>{description}</span>
    )
  ) : null;

  return (
    <div className={pageRhythm.pageTitleBlock} data-help="page-title">
      <h1 className={cn(pageRhythm.pageTitle, 'shrink-0')}>{title}</h1>
      {badge ? <span className="self-center">{badge}</span> : null}
      {meta}
    </div>
  );
}

/**
 * 页头：只把标题/说明登记到顶栏，自身不占位。
 * 页内操作放进 `chromeRow` 右侧（`chromeActions`），不要再单独占一行。
 * 需要补充说明时用 descriptionTip（悬停展示），勿把长说明直接铺在页面上。
 */
export function PageHeader({
  title,
  badge,
  description,
  descriptionTip,
}: {
  title: string;
  /** 标题旁状态标记（如更新 pin / 运行状态） */
  badge?: ReactNode;
  description?: string;
  /** 悬停时的详细说明；有则 description 可更短 */
  descriptionTip?: string;
}) {
  useRegisterPageChrome({ title, description, descriptionTip, badge });
  return null;
}
