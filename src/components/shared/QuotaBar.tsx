import { Progress } from '@/components/ui/progress';
import { cn } from '@/lib/utils';

/** 5h/7d 配额窗口进度条 + reset 倒计时(docs/ui-design.md §5 QuotaBar) */
export function QuotaBar({
  label,
  pct,
  resetIn,
}: {
  label: string;
  pct?: number;
  resetIn?: string;
}) {
  if (pct === undefined) return null;
  const color = pct >= 90 ? 'bg-danger' : pct >= 70 ? 'bg-warning' : 'bg-success';
  return (
    <div className="flex items-center gap-2">
      <span className="w-8 shrink-0 text-meta text-muted">{label}</span>
      <Progress value={pct} className="h-1.5 w-20" indicatorClassName={cn(color)} />
      <span className="text-meta text-secondary">{pct}%</span>
      {resetIn && <span className="text-meta text-muted">{resetIn}</span>}
    </div>
  );
}
