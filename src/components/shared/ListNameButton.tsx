import type { ButtonHTMLAttributes, ReactNode } from 'react';
import { Tip } from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';

/** Dense-table inspect affordance: the name is the only hit target. */
export const LIST_NAME_BUTTON_CLASS =
  'max-w-full truncate text-left text-body font-medium text-primary hover:underline rounded-btn focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60';

/**
 * Click-the-name control for field tables (Agents / Connections / Pool).
 * Do not use `Button` here: padding and height would break table cells.
 * Simple inventories without row chrome should use `TableRow onOpen` / `ListRow onOpen` instead.
 */
export function ListNameButton({
  hint,
  className,
  children,
  type = 'button',
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  hint?: ReactNode;
  children: ReactNode;
}) {
  const button = (
    <button type={type} className={cn(LIST_NAME_BUTTON_CLASS, className)} {...props}>
      {children}
    </button>
  );
  if (hint == null || hint === '') return button;
  return (
    <Tip className="min-w-0" label={hint}>
      {button}
    </Tip>
  );
}
