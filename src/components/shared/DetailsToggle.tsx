import type { ReactNode } from 'react';
import { ChevronDown } from 'lucide-react';
import { Button, type ButtonProps } from '@/components/ui/button';
import { cn } from '@/lib/utils';

/**
 * Row 详情 expand/collapse. Same ghost + chevron control on
 * MCP and import-login rows. Routes and Connections open a side pane
 * and use a plain outline button instead.
 */
export function DetailsToggle({
  open,
  controlsId,
  children,
  className,
  type = 'button',
  ...props
}: {
  open: boolean;
  controlsId?: string;
  children: ReactNode;
} & Omit<ButtonProps, 'children' | 'aria-expanded' | 'aria-controls'>) {
  return (
    <Button
      type={type}
      className={cn('shrink-0', className)}
      {...props}
      size="sm"
      variant="ghost"
      aria-expanded={open}
      aria-controls={controlsId}
    >
      {children}
      <ChevronDown
        className={cn('h-3.5 w-3.5 shrink-0 transition-transform', open && 'rotate-180')}
        aria-hidden
      />
    </Button>
  );
}
