import { Card } from '@/components/ui/card';
import { tableStyles } from '@/components/ui/table';
import { cn } from '@/lib/utils';

export function Skeleton({ className }: { className?: string }) {
  return <div className={cn('animate-pulse rounded-btn bg-subtle', className)} />;
}

/** 列表卡片骨架：与 AccountCard / AgentCard 行高对齐，loading → 内容零跳动 */
export function ListSkeleton({ rows = 3, className }: { rows?: number; className?: string }) {
  return (
    <div className={cn('flex flex-col gap-3', className)}>
      {Array.from({ length: rows }).map((_, i) => (
        <Card key={i} className="p-3">
          <div className="flex items-center gap-3">
            <Skeleton className="h-2 w-2 rounded-full" />
            <Skeleton className="h-4 w-40" />
            <Skeleton className="h-5 w-16" />
            <div className="ml-auto flex items-center gap-3">
              <Skeleton className="h-1.5 w-20" />
              <Skeleton className="h-7 w-14" />
            </div>
          </div>
          <Skeleton className="mt-2 h-3 w-48" />
        </Card>
      ))}
    </div>
  );
}

/** 指标/网格卡片骨架 */
export function CardGridSkeleton({
  count = 4,
  className,
}: {
  count?: number;
  className?: string;
}) {
  return (
    <div className={cn('grid grid-cols-2 gap-3 lg:grid-cols-4', className)}>
      {Array.from({ length: count }).map((_, i) => (
        <Card key={i} className="p-3">
          <Skeleton className="h-3 w-16" />
          <Skeleton className="mt-2 h-6 w-24" />
        </Card>
      ))}
    </div>
  );
}

/** 表格骨架：与 ui/table 表头/行密度对齐，loading → 内容少跳动 */
export function TableSkeleton({
  rows = 8,
  cols = 6,
  className,
  /** 与 TableShell variant 对齐：Skills 用 workbench（无 Card 壳） */
  variant = 'default',
}: {
  rows?: number;
  cols?: number;
  className?: string;
  variant?: 'default' | 'workbench';
}) {
  const workbench = variant === 'workbench';
  const head = workbench ? tableStyles.theadRowWorkbench : tableStyles.theadRow;
  const rowBorder = workbench ? 'border-t border-border/40' : 'border-t border-border/50';
  const body = (
    <>
      <div className={cn('flex gap-3 px-3 py-2', head)}>
        {Array.from({ length: cols }).map((_, i) => (
          <Skeleton key={i} className="h-3 flex-1" />
        ))}
      </div>
      {Array.from({ length: rows }).map((_, r) => (
        <div
          key={r}
          className={cn('flex items-center gap-3 px-3 py-2 last:border-0', rowBorder)}
        >
          {Array.from({ length: cols }).map((_, c) => (
            <Skeleton key={c} className="h-3.5 flex-1" />
          ))}
        </div>
      ))}
    </>
  );

  if (workbench) {
    return (
      <div className={cn('overflow-hidden', className)} data-table-shell="workbench">
        {body}
      </div>
    );
  }

  return <Card className={cn('overflow-hidden', className)}>{body}</Card>;
}
