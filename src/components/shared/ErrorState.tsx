import { AlertTriangle, Copy, RefreshCw } from 'lucide-react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import { useToast } from '@/components/ui/toast';
import { cn } from '@/lib/utils';

/** 错误态:错误摘要 + 重试(主按钮) + 复制诊断信息(docs/ui-design.md §6) */
export function ErrorState({
  error,
  onRetry,
  title,
  hint,
  compact = false,
  className,
}: {
  error: unknown;
  onRetry: () => void;
  title?: string;
  /** Optional plain-language next step under the message (e.g. log path). */
  hint?: string;
  /** 分区内嵌错误（如 Dashboard 明细段），降低垂直占用 */
  compact?: boolean;
  className?: string;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const resolvedTitle = title ?? t('chrome.error.title');
  const message = error instanceof Error ? error.message : String(error);
  const resolvedHint = hint ?? (compact ? '' : t('chrome.error.hintLogs'));
  const diag = [
    t('chrome.error.diagHeader'),
    t('chrome.error.diagTime', { time: new Date().toISOString() }),
    t('chrome.error.diagError', { message }),
    t('chrome.error.diagLogs'),
  ].join('\n');

  return (
    <div
      className={cn(
        'flex flex-col items-center justify-center gap-3 rounded-card border border-danger/30 bg-danger/5 text-center',
        compact ? 'gap-2 px-3 py-4' : 'py-10',
        className,
      )}
    >
      <AlertTriangle className={cn('text-danger', compact ? 'h-5 w-5' : 'h-6 w-6')} />
      <p className="text-title font-medium">{resolvedTitle}</p>
      <p className="max-w-md text-meta text-secondary">{message}</p>
      {resolvedHint ? (
        <p className="max-w-md text-meta text-muted">{resolvedHint}</p>
      ) : null}
      <div className="flex gap-2">
        <Button size="sm" variant="default" onClick={onRetry}>
          <RefreshCw className="h-3.5 w-3.5" /> {t('chrome.error.retry')}
        </Button>
        <Button
          size="sm"
          variant="outline"
          onClick={() => {
            navigator.clipboard.writeText(diag).catch(() => {});
            toast({ title: t('chrome.error.copied') });
          }}
        >
          <Copy className="h-3.5 w-3.5" /> {t('chrome.error.copyDiag')}
        </Button>
      </div>
    </div>
  );
}
