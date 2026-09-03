import type { MouseEvent as ReactMouseEvent } from 'react';
import { Copy } from 'lucide-react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Hint } from '@/components/ui/tooltip';
import { useToast } from '@/components/ui/toast';
import { cn } from '@/lib/utils';

export function copyTextToClipboard(text: string): Promise<void> {
  return navigator.clipboard.writeText(text);
}

export function CopyTextButton({
  text,
  label,
  className,
}: {
  text: string;
  label?: string;
  className?: string;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const aria = label ?? t('common.copyMessage');
  if (!text.trim()) return null;

  const onClick = (e: ReactMouseEvent) => {
    e.stopPropagation();
    void copyTextToClipboard(text).then(
      () => toast({ title: t('common.copied'), variant: 'success' }),
      () => toast({ title: t('common.copyFailed'), variant: 'danger' }),
    );
  };

  return (
    <Hint label={aria}>
      <button
        type="button"
        aria-label={aria}
        className={cn(
          'absolute bottom-1 right-1 rounded-btn p-1 text-muted',
          'opacity-0 transition-opacity hover:bg-panel hover:text-primary',
          'group-hover:opacity-100 focus-visible:opacity-100 group-focus-within:opacity-100',
          className,
        )}
        onClick={onClick}
      >
        <Copy className="h-3.5 w-3.5" />
      </button>
    </Hint>
  );
}
