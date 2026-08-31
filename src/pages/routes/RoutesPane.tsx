import type { ReactNode } from 'react';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { cn } from '@/lib/utils';

/** 路由区非分栏子页滚动壳：左右缘与列表工作台对齐。 */
export function RoutesPane({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        'h-full min-h-0 overflow-y-auto',
        pageRhythm.workbenchX,
        pageRhythm.workbenchPadT,
        pageRhythm.workbenchY,
        className,
      )}
      data-routes-pane
    >
      {children}
    </div>
  );
}
