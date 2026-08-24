import type { ReactNode } from 'react';
import { SideInspectPanel } from '@/components/layout/SideInspectPanel';
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
  footer,
  preventDismiss,
}: {
  asPanel?: boolean;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description?: string;
  children: ReactNode;
  footer: ReactNode;
  preventDismiss?: boolean;
}) {
  if (asPanel) {
    if (!open) return null;
    return (
      <SideInspectPanel title={title} description={description} onClose={() => onOpenChange(false)} footer={footer}>
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
        <DialogFooter className="mt-4 shrink-0 border-t border-border pt-4">{footer}</DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
