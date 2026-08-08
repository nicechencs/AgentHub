import * as React from 'react';
import { Search } from 'lucide-react';
import { Input } from '@/components/ui/input';
import { cn } from '@/lib/utils';

/**
 * 统一搜索框：h-7 + 左侧图标 inset，与全局 Input 密度一致。
 * 页面禁止再手写 relative + Search icon + pl-8。
 */
export function SearchField({
  className,
  inputClassName,
  ...props
}: Omit<React.ComponentPropsWithoutRef<typeof Input>, 'className'> & {
  className?: string;
  inputClassName?: string;
}) {
  return (
    <div className={cn('relative min-w-0', className)}>
      <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted" />
      <Input className={cn('pl-8', inputClassName)} {...props} />
    </div>
  );
}
