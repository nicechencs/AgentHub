import * as React from 'react';
import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from '@/lib/utils';

/**
 * status: 无边框、淡底、语义文字色 —— 表达状态
 * chip: 有边框、可点击 affordance —— 表达可选项
 */
const badgeVariants = cva(
  'inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-xs font-medium transition-colors',
  {
    variants: {
      variant: {
        default: 'border-transparent bg-subtle text-secondary',
        accent: 'border-transparent bg-subtle text-accent',
        success: 'border-transparent bg-subtle text-success',
        warning: 'border-transparent bg-subtle text-warning',
        danger: 'border-transparent bg-subtle text-danger',
        info: 'border-transparent bg-subtle text-info',
        chip: 'cursor-pointer border border-border bg-panel text-secondary hover:bg-hover',
        chipActive:
          'cursor-pointer border border-border-strong bg-hover font-medium text-primary',
      },
    },
    defaultVariants: { variant: 'default' },
  },
);

export interface BadgeProps
  extends React.HTMLAttributes<HTMLSpanElement>,
    VariantProps<typeof badgeVariants> {}

export function Badge({ className, variant, ...props }: BadgeProps) {
  return <span className={cn(badgeVariants({ variant }), className)} {...props} />;
}
