import { useState, type MouseEvent as ReactMouseEvent } from 'react';
import { Check, Copy } from 'lucide-react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import { useToast } from '@/components/ui/toast';
import { cn } from '@/lib/utils';

export function copyTextToClipboard(text: string): Promise<void> {
  return navigator.clipboard.writeText(text);
}

/**
 * One copy control: ghost button, icon, and 复制 / 已复制.
 * `toolbar` stays visible on a snippet or preview header.
 * `hover` sits on a message and appears with the row.
 */
export function ContentCopyButton({
  text,
  label,
  placement = 'toolbar',
  className,
  disabled = false,
}: {
  text: string;
  /** Accessible name and, when set, the visible label. Defaults to 复制. */
  label?: string;
  placement?: 'toolbar' | 'hover';
  className?: string;
  disabled?: boolean;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const [copied, setCopied] = useState(false);
  const idle = label ?? t('common.copy');
  const onClick = (e: ReactMouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (!text.trim() || disabled) return;
    void copyTextToClipboard(text).then(
      () => {
        setCopied(true);
        window.setTimeout(() => setCopied(false), 1200);
      },
      () => toast({ title: t('common.copyFailed'), variant: 'danger' }),
    );
  };

  return (
    <Button
      type="button"
      size="sm"
      variant="ghost"
      className={cn(
        'h-7 shrink-0 px-2',
        placement === 'hover' && [
          'absolute bottom-1 right-1',
          'opacity-0 transition-opacity group-hover:opacity-100',
          'focus-visible:opacity-100 group-focus-within:opacity-100',
          copied && 'opacity-100',
        ],
        className,
      )}
      aria-label={copied ? t('common.copied') : idle}
      disabled={disabled || !text.trim()}
      onClick={onClick}
      onPointerDown={(e) => e.stopPropagation()}
    >
      {copied ? <Check className="h-3.5 w-3.5 text-success" /> : <Copy className="h-3.5 w-3.5" />}
      {copied ? t('common.copied') : idle}
    </Button>
  );
}

/** Message-row copy. Same button as snippet toolbars, revealed on hover. */
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
  if (!text.trim()) return null;
  return (
    <ContentCopyButton
      text={text}
      label={label ?? t('common.copyMessage')}
      placement="hover"
      className={className}
    />
  );
}
