import { AlertTriangle, Copy, RefreshCw } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { useToast } from '@/components/ui/toast';
import { cn } from '@/lib/utils';

/** 错误态:错误摘要 + 重试 + 复制诊断信息(docs/ui-design.md §6) */
export function ErrorState({
  error,
  onRetry,
  title = '加载失败',
  compact = false,
  className,
}: {
  error: unknown;
  onRetry: () => void;
  title?: string;
  /** 分区内嵌错误（如 Dashboard 明细段），降低垂直占用 */
  compact?: boolean;
  className?: string;
}) {
  const { toast } = useToast();
  const message = error instanceof Error ? error.message : String(error);
  const diag = `[AgentHub 诊断]\n时间: ${new Date().toISOString()}\n错误: ${message}`;

  return (
    <div
      className={cn(
        'flex flex-col items-center justify-center gap-3 rounded-card border border-danger/30 bg-danger/5 text-center',
        compact ? 'gap-2 px-3 py-4' : 'py-10',
        className,
      )}
    >
      <AlertTriangle className={cn('text-danger', compact ? 'h-5 w-5' : 'h-6 w-6')} />
      <p className="text-sm font-medium">{title}</p>
      <p className="max-w-md text-xs text-secondary">{message}</p>
      <div className="flex gap-2">
        <Button size="sm" variant="secondary" onClick={onRetry}>
          <RefreshCw className="h-3.5 w-3.5" /> 重试
        </Button>
        <Button
          size="sm"
          variant="outline"
          onClick={() => {
            navigator.clipboard.writeText(diag).catch(() => {});
            toast({ title: '诊断信息已复制' });
          }}
        >
          <Copy className="h-3.5 w-3.5" /> 复制诊断信息
        </Button>
      </div>
    </div>
  );
}
