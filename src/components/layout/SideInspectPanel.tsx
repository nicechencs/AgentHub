import { PanelRightClose } from 'lucide-react';
import { useEffect, type ReactNode, type TransitionEvent as ReactTransitionEvent } from 'react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import { hasEscPriorityOverlay } from '@/lib/skills/preview-keys';
import { cn } from '@/lib/utils';

/**
 * Right-hand inspect pane — same card chrome as Skills / Projects previews:
 * h-10 header, primary actions + collapse on the right, optional footer.
 */
export function SideInspectPanel({
  title,
  description,
  onClose,
  children,
  headerActions,
  footer,
  width,
  className,
  onWidthTransitionEnd,
}: {
  title: string;
  description?: string;
  onClose: () => void;
  children: ReactNode;
  headerActions?: ReactNode;
  footer?: ReactNode;
  width?: number;
  className?: string;
  onWidthTransitionEnd?: (e: ReactTransitionEvent<HTMLElement>) => void;
}) {
  const { t } = useI18n();
  const collapseLabel = t('common.collapse');

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      if (hasEscPriorityOverlay()) return;
      e.preventDefault();
      onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  return (
    <aside
      className={cn(
        'flex h-full min-h-0 min-w-0 shrink-0 flex-col overflow-hidden rounded-card border border-border bg-panel shadow-xs',
        width == null && 'w-[min(26rem,100%)]',
        className,
      )}
      style={width != null ? { width } : undefined}
      data-side-inspect=""
      data-help="inspect-panel"
      onTransitionEnd={onWidthTransitionEnd}
    >
      <header className="flex h-10 shrink-0 items-center gap-1.5 overflow-x-auto border-b border-border px-3">
        <div className="min-w-0 flex-1 basis-16">
          <div className="flex min-w-0 items-baseline gap-2">
            <h2 className="truncate text-body font-semibold leading-tight text-primary">{title}</h2>
            {description ? (
              <span className="min-w-0 truncate text-meta text-muted">{description}</span>
            ) : null}
          </div>
        </div>
        {headerActions ? <div className="flex shrink-0 items-center gap-1.5">{headerActions}</div> : null}
        <Button
          size="icon"
          variant="ghost"
          className="h-7 w-7 shrink-0"
          aria-label={collapseLabel}
          title={collapseLabel}
          onClick={onClose}
        >
          <PanelRightClose className="h-4 w-4" />
        </Button>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain px-4 py-3">{children}</div>
      {footer ? (
        <div className="flex shrink-0 flex-wrap items-center justify-start gap-2 border-t border-border px-3 py-2">
          {footer}
        </div>
      ) : null}
    </aside>
  );
}
