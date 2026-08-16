import * as React from 'react';
import { Hint } from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';

export interface InputProps extends React.InputHTMLAttributes<HTMLInputElement> {
  /**
   * 悬停提示。走 Radix Tooltip，不用浏览器原生 title。
   */
  title?: string;
}

const Input = React.forwardRef<HTMLInputElement, InputProps>(
  ({ className, title, ...props }, ref) => {
    const input = (
      <input
        ref={ref}
        className={cn(
          'flex h-7 w-full rounded-btn border border-border-strong bg-panel px-2.5 text-body text-primary placeholder:text-muted focus:outline-none focus:ring-2 focus:ring-accent/60 disabled:opacity-50',
          className,
        )}
        {...props}
      />
    );
    if (!title) return input;
    return <Hint label={title}>{input}</Hint>;
  },
);
Input.displayName = 'Input';

export { Input };
