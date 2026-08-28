import * as React from 'react';
import { Check, Copy } from 'lucide-react';
import { cn } from '@/lib/utils';

/** Label + value pair used in expandable connection/account detail grids. */
export function DetailRow({
  label,
  value,
  mono,
  copyable,
  className,
}: {
  label: string;
  value: string;
  mono?: boolean;
  copyable?: boolean;
  className?: string;
}) {
  const [copied, setCopied] = React.useState(false);

  const onCopy = React.useCallback(() => {
    void navigator.clipboard.writeText(value).then(() => {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1200);
    }).catch(() => {});
  }, [value]);

  return (
    <span className={cn('flex min-w-0 items-start gap-1.5', className)}>
      <span className="min-w-0 flex-1">
        <span className="text-muted">{label} </span>
        {mono ? (
          <code className="break-all font-mono text-secondary">{value}</code>
        ) : (
          <span className="break-all text-secondary">{value}</span>
        )}
      </span>
      {copyable ? (
        <button
          type="button"
          className="mt-0.5 shrink-0 rounded p-0.5 text-muted hover:bg-subtle hover:text-secondary"
          aria-label={`Copy ${label}`}
          onClick={onCopy}
        >
          {copied ? <Check className="h-3 w-3 text-success" /> : <Copy className="h-3 w-3" />}
        </button>
      ) : null}
    </span>
  );
}
