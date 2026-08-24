import { PanelRightClose } from 'lucide-react';
import type { ReactNode } from 'react';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

/**
 * Right-hand inspect pane, same idea as the skills SKILL.md preview:
 * list stays on the left, long forms open here instead of a center dialog.
 */
export function SideInspectPanel({
  title,
  description,
  onClose,
  children,
  footer,
  className,
}: {
  title: string;
  description?: string;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  className?: string;
}) {
  return (
    <aside
      className={cn(
        'flex h-full min-h-[28rem] w-[min(26rem,100%)] shrink-0 flex-col overflow-hidden rounded-card border border-border bg-background',
        className,
      )}
      data-side-inspect=""
    >
      <div className="flex shrink-0 items-start justify-between gap-2 border-b border-border px-3 py-2.5">
        <div className="min-w-0">
          <h2 className="text-body font-medium">{title}</h2>
          {description ? <p className="mt-0.5 text-meta text-muted">{description}</p> : null}
        </div>
        <Button
          size="icon"
          variant="ghost"
          className="h-7 w-7 shrink-0"
          aria-label="收起"
          title="收起"
          onClick={onClose}
        >
          <PanelRightClose className="h-4 w-4" />
        </Button>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain px-3 py-3">{children}</div>
      {footer ? (
        <div className="flex shrink-0 justify-end gap-2 border-t border-border px-3 py-3">{footer}</div>
      ) : null}
    </aside>
  );
}
