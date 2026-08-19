import * as React from 'react';
import { cva, type VariantProps } from 'class-variance-authority';
import { Hint } from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';

/**
 * Action button — one family, six roles, two heights.
 *
 * Hover / press = fill + text only. `shadow-none` is locked so a page
 * `shadow-*` cannot leak a hover lift onto some buttons and not others.
 * Segmented selected lift and overlay FABs are not this component.
 *
 * Roles: default (page CTA, ≤1) · secondary · outline · ghost · danger · dangerOutline
 * Sizes: sm/default/icon = 28 · lg = 32. Padding is 8 / 12 / 16.
 */
const buttonVariants = cva(
  [
    'inline-flex items-center justify-center gap-1.5 whitespace-nowrap',
    'rounded-btn text-body font-medium cursor-pointer',
    'shadow-none hover:shadow-none active:shadow-none',
    'transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60',
    'disabled:pointer-events-none disabled:opacity-50',
  ].join(' '),
  {
    variants: {
      variant: {
        default: 'bg-accent text-white hover:bg-accent/90 active:bg-accent/80',
        secondary: 'bg-subtle text-primary hover:bg-hover active:bg-active',
        outline:
          'border border-border bg-transparent text-secondary hover:bg-hover hover:text-primary active:bg-active',
        ghost: 'text-secondary hover:bg-hover hover:text-primary active:bg-active',
        danger: 'bg-danger text-white hover:bg-danger/90 active:bg-danger/80',
        dangerOutline:
          'border border-danger/40 text-danger hover:bg-danger/10 active:bg-danger/15',
      },
      size: {
        default: 'h-7 px-3',
        sm: 'h-7 px-2 text-meta',
        lg: 'h-8 px-4',
        icon: 'h-7 w-7',
      },
    },
    defaultVariants: { variant: 'default', size: 'default' },
  },
);

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  /**
   * 悬停提示。走 Radix Tooltip（相对按钮上方定位），不用浏览器原生 title，
   * 避免被鼠标指针遮挡。作为 DropdownMenuTrigger asChild 子节点时请勿设 title，
   * 改在外侧包 Hint。
   */
  title?: string;
}

const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, title, ...props }, ref) => {
    const resolvedVariant = variant ?? 'default';
    const resolvedSize = size ?? 'default';
    const button = (
      <button
        className={cn(buttonVariants({ variant: resolvedVariant, size: resolvedSize }), className)}
        data-btn={resolvedVariant}
        data-btn-size={resolvedSize}
        ref={ref}
        {...props}
      />
    );
    if (!title) return button;
    return <Hint label={title}>{button}</Hint>;
  },
);
Button.displayName = 'Button';

export { Button, buttonVariants };
