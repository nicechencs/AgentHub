import type { ReactNode } from 'react';
import { SideInspectPanel } from '@/components/layout/SideInspectPanel';
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

export function DialogOrSide({
  asPanel,
  open,
  onOpenChange,
  title,
  description,
  children,
  primary,
  danger,
  preventDismiss,
  width,
}: {
  asPanel?: boolean;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description?: string;
  children: ReactNode;
  /** Save / submit — header on the panel, footer on the dialog. */
  primary?: ReactNode;
  /** Destructive action — footer on both. */
  danger?: ReactNode;
  preventDismiss?: boolean;
  width?: number;
}) {
  const { t } = useI18n();
  const cancel = (
    <Button type="button" variant="outline" size="sm" onClick={() => onOpenChange(false)}>
      {t('common.cancel')}
    </Button>
  );
  const actions = (
    <>
      {cancel}
      {primary}
    </>
  );
  if (asPanel) {
    if (!open) return null;
    return (
      <SideInspectPanel
        title={title}
        description={description}
        onClose={() => onOpenChange(false)}
        headerActions={actions}
        footer={danger}
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
        <div className="min-h-0 flex-1 space-y-2 overflow-y-auto overscroll-contain pr-1 pb-1">{children}</div>
        <DialogFooter className="mt-4 shrink-0 border-t border-border pt-4">
          {danger}
          {actions}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
