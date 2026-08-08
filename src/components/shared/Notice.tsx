import type { ReactNode } from 'react';
import { AlertTriangle, CheckCircle2, Info, X } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

const TONE = {
  neutral: {
    wrap: 'border-border bg-panel',
    icon: 'text-muted',
    Icon: Info,
  },
  info: {
    wrap: 'border-border bg-subtle',
    icon: 'text-muted',
    Icon: Info,
  },
  warning: {
    wrap: 'border-warning/30 bg-warning/5',
    icon: 'text-warning',
    Icon: AlertTriangle,
  },
  danger: {
    wrap: 'border-danger/30 bg-danger/5',
    icon: 'text-danger',
    Icon: AlertTriangle,
  },
  success: {
    wrap: 'border-success/30 bg-success/5',
    icon: 'text-success',
    Icon: CheckCircle2,
  },
} as const;

export type NoticeTone = keyof typeof TONE;

/** 统一信息条：一屏建议最多一条；页面横幅应优先走本组件 */
export function Notice({
  children,
  className,
  tone = 'neutral',
  actionLabel,
  onAction,
  onDismiss,
}: {
  children: ReactNode;
  className?: string;
  tone?: NoticeTone;
  actionLabel?: string;
  onAction?: () => void;
  onDismiss?: () => void;
}) {
  const conf = TONE[tone];
  const Icon = conf.Icon;

  return (
    <div
      className={cn(
        'flex items-start gap-2 rounded-card border px-3 py-2 text-xs leading-relaxed text-secondary',
        conf.wrap,
        className,
      )}
    >
      <Icon className={cn('mt-0.5 h-3.5 w-3.5 shrink-0', conf.icon)} />
      <div className="min-w-0 flex-1">{children}</div>
      {(actionLabel && onAction) || onDismiss ? (
        <div className="flex shrink-0 items-center gap-1">
          {actionLabel && onAction && (
            <Button size="sm" variant="outline" className="h-6 px-2 text-xs" onClick={onAction}>
              {actionLabel}
            </Button>
          )}
          {onDismiss && (
            <button
              type="button"
              onClick={onDismiss}
              className="rounded-btn p-1 text-muted hover:bg-hover hover:text-primary"
              aria-label="关闭"
            >
              <X className="h-3.5 w-3.5" />
            </button>
          )}
        </div>
      ) : null}
    </div>
  );
}
