import { RefreshCw } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

/**
 * 页头「刷新这一页的列表」。灰底 + 刷新图标 + 文案；忙时同一图标转圈。
 * 出错重试、空列表刷新、刷新这一份登录不要用这个。
 */
export function PageRefreshButton({
  loading = false,
  disabled,
  onClick,
  label,
  className,
}: {
  loading?: boolean;
  disabled?: boolean;
  onClick: () => void;
  label: string;
  className?: string;
}) {
  return (
    <Button
      type="button"
      size="sm"
      variant="secondary"
      data-help="page-refresh"
      className={cn(className)}
      disabled={disabled || loading}
      onClick={onClick}
    >
      <RefreshCw className={cn('h-3.5 w-3.5', loading && 'animate-spin')} />
      {label}
    </Button>
  );
}
