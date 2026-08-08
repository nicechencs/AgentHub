import * as React from 'react';
import { cva, type VariantProps } from 'class-variance-authority';
import { Hint } from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';

const buttonVariants = cva(
  'inline-flex items-center justify-center gap-1.5 whitespace-nowrap rounded-btn text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60 disabled:pointer-events-none disabled:opacity-50',
  {
    variants: {
      variant: {
        default: 'bg-accent text-white hover:bg-accent/90',
        secondary: 'bg-subtle text-primary hover:bg-hover',
        outline:
          'border border-border bg-transparent text-secondary hover:bg-hover hover:text-primary',
        ghost: 'text-secondary hover:bg-hover hover:text-primary',
        danger: 'bg-danger text-white hover:bg-danger/90',
        dangerOutline: 'border border-danger/40 text-danger hover:bg-danger/10',
      },
      size: {
        default: 'h-7 px-3',
        sm: 'h-7 px-2.5 text-xs',
        lg: 'h-8 px-3.5',
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
    const button = (
      <button
        className={cn(buttonVariants({ variant, size, className }))}
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
