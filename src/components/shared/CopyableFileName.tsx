import { useI18n } from '@/components/shared/LanguageProvider';
import { splitFileLabel } from '@/components/shared/file-name-label';
import { Tip } from '@/components/ui/tooltip';
import { useToast } from '@/components/ui/toast';
import { cn } from '@/lib/utils';

/**
 * One-line path: muted directory + emphasized file name.
 * Click copies the file name and toasts.
 */
export function CopyableFileName({
  path,
  name,
  wrap = 'truncate',
  align = 'start',
  className,
}: {
  path: string;
  name?: string;
  wrap?: 'truncate' | 'break';
  align?: 'start' | 'end';
  className?: string;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const { directory, fileName } = splitFileLabel(path, name ?? '');
  const display = fileName.trim();
  if (!display && !directory) return null;

  const onCopy = () => {
    if (!display) return;
    void navigator.clipboard.writeText(display).then(() => {
      toast({ title: t('common.copiedFileName', { name: display }), variant: 'success' });
    }).catch(() => {});
  };

  return (
    <Tip label={path.trim() || display} className={cn('min-w-0', className)}>
      <button
        type="button"
        className={cn(
          'max-w-full text-left font-mono text-meta hover:[&>span:last-child]:text-accent',
          wrap === 'truncate' && 'flex min-w-0 items-baseline',
          wrap === 'break' && 'break-all',
          align === 'end' && 'ml-auto',
        )}
        aria-label={t('common.copyFileName')}
        onClick={onCopy}
      >
        {directory ? (
          <span className={cn('text-muted', wrap === 'truncate' && 'min-w-0 truncate')}>
            {directory}
          </span>
        ) : null}
        <span className={cn('font-medium text-primary', wrap === 'truncate' && 'shrink-0')}>
          {display}
        </span>
      </button>
    </Tip>
  );
}
