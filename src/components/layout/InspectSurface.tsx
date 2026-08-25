import type { ReactNode } from 'react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { SideInspectPanel } from './SideInspectPanel';

/** Shared surface for inspect/edit content in a right pane or modal dialog. */
export type InspectSurfaceProps = {
  asPanel?: boolean;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description?: string;
  children: ReactNode;
  /** Save / submit — header on the panel, footer on the dialog. */
  primary?: ReactNode;
  /** Destructive action — header on the panel (with cancel), footer on the dialog. */
  danger?: ReactNode;
  /**
   * Form surfaces keep Cancel. Read-only inspect should pass false so the
   * only close control is the shared collapse button.
   */
  showCancel?: boolean;
  preventDismiss?: boolean;
  width?: number;
};

export function InspectSurface({
  asPanel,
  open,
  onOpenChange,
  title,
  description,
  children,
  primary,
  danger,
  showCancel = true,
  preventDismiss,
  width,
}: InspectSurfaceProps) {
  const { t } = useI18n();
  const cancel = showCancel ? (
    <Button type="button" variant="secondary" size="sm" onClick={() => onOpenChange(false)}>
      {t('common.cancel')}
    </Button>
  ) : null;

  if (asPanel) {
    if (!open) return null;
    return (
      <SideInspectPanel
        title={title}
        description={description}
        onClose={() => onOpenChange(false)}
        headerActions={
          <>
            {cancel}
            {danger}
            {primary}
          </>
        }
        width={width}
      >
        {children}
      </SideInspectPanel>
    );
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="flex max-h-[min(36rem,calc(100vh-2rem))] flex-col overflow-hidden"
        onPointerDownOutside={preventDismiss ? (event) => event.preventDefault() : undefined}
        onInteractOutside={preventDismiss ? (event) => event.preventDefault() : undefined}
        onFocusOutside={preventDismiss ? (event) => event.preventDefault() : undefined}
      >
        <DialogHeader className="shrink-0">
          <DialogTitle>{title}</DialogTitle>
          {description ? <DialogDescription>{description}</DialogDescription> : null}
        </DialogHeader>
        <div className="min-h-0 flex-1 space-y-2 overflow-y-auto overscroll-contain pr-1 pb-1">
          {children}
        </div>
        <DialogFooter className="mt-4 shrink-0 border-t border-border pt-4">
          {danger}
          {cancel}
          {primary}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
