import * as React from 'react';
import { Search, X } from 'lucide-react';
import { Input } from '@/components/ui/input';
import { cn } from '@/lib/utils';

/**
 * 统一搜索框：h-7 + 左侧图标 inset，与全局 Input 密度一致。
 * 页面禁止再手写 relative + Search icon + pl-8。
 */
export function SearchField({
  className,
  inputClassName,
  value,
  onChange,
  onKeyDown,
  clearLabel = '清空搜索',
  ...props
}: Omit<React.ComponentPropsWithoutRef<typeof Input>, 'className'> & {
  className?: string;
  inputClassName?: string;
  clearLabel?: string;
}) {
  const hasValue = String(value ?? '').length > 0;
  const clear = () => {
    onChange?.({
      target: { value: '' },
      currentTarget: { value: '' },
    } as React.ChangeEvent<HTMLInputElement>);
  };
  return (
    <div className={cn('relative min-w-0', className)}>
      <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted" />
      <Input
        className={cn('pl-8', hasValue && 'pr-8', inputClassName)}
        value={value}
        onChange={onChange}
        onKeyDown={(event) => {
          onKeyDown?.(event);
          if (!event.defaultPrevented && event.key === 'Escape' && hasValue) {
            event.preventDefault();
            clear();
          }
        }}
        {...props}
      />
      {hasValue && !props.disabled ? (
        <button
          type="button"
          className="absolute right-1.5 top-1/2 flex h-5 w-5 -translate-y-1/2 items-center justify-center rounded-btn text-muted hover:bg-hover hover:text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
          aria-label={clearLabel}
          onClick={clear}
        >
          <X className="h-3.5 w-3.5" />
        </button>
      ) : null}
    </div>
  );
}
