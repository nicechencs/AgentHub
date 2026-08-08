import * as React from 'react';
import * as SwitchPrimitive from '@radix-ui/react-switch';
import { cn } from '@/lib/utils';

const Switch = React.forwardRef<
  React.ElementRef<typeof SwitchPrimitive.Root>,
  React.ComponentPropsWithoutRef<typeof SwitchPrimitive.Root>
>(({ className, ...props }, ref) => (
  <SwitchPrimitive.Root
    ref={ref}
    className={cn(
      'peer relative inline-flex h-6 w-10 shrink-0 cursor-pointer items-center rounded-full border-0 bg-transparent p-0 transition-colors',
      'before:absolute before:inset-x-1 before:top-1/2 before:h-4 before:w-7 before:-translate-y-1/2 before:rounded-full before:border before:border-border before:bg-subtle before:content-[""]',
      // primary 在本项目是文字色；开关选中态用 accent
      'data-[state=checked]:before:border-accent/40 data-[state=checked]:before:bg-accent',
      'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60',
      className,
    )}
    {...props}
  >
    <SwitchPrimitive.Thumb
      className={cn(
        'relative z-10 block h-3 w-3 translate-x-1 rounded-full bg-panel shadow-xs transition-transform',
        'data-[state=checked]:translate-x-6',
      )}
    />
  </SwitchPrimitive.Root>
));
Switch.displayName = 'Switch';

export { Switch };
