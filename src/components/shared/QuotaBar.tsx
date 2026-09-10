import { Progress } from '@/components/ui/progress';
import { cn } from '@/lib/utils';

/** 5h/7d 配额窗口进度条 + reset 倒计时(docs/ui-design.md §5 QuotaBar) */
export function QuotaBar({
  label,
  pct,
  resetIn,
  compact = false,
  showLabel = true,
}: {
  label: string;
  pct?: number;
  resetIn?: string;
  /** Table cells: fill remaining width, omit reset copy. */
  compact?: boolean;
  /** When the label already lives in a row header, hide the in-bar copy. */
  showLabel?: boolean;
}) {
  if (pct === undefined) return null;
  const color = pct >= 90 ? 'bg-danger' : pct >= 70 ? 'bg-warning' : 'bg-success';
  return (
    <div className={cn('flex min-w-0 items-center', compact ? 'gap-1.5' : 'gap-2')}>
      {showLabel ? (
        <span className={cn('shrink-0 text-meta text-muted', compact ? undefined : 'w-8')}>
          {label}
        </span>
      ) : null}
      <Progress
        value={pct}
        className={cn('h-1.5', compact ? 'min-w-0 flex-1' : 'w-20')}
        indicatorClassName={cn(color)}
      />
      <span className="shrink-0 text-meta text-secondary tabular-nums">{pct}%</span>
      {resetIn && !compact ? <span className="text-meta text-muted">{resetIn}</span> : null}
    </div>
  );
}
