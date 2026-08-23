import * as React from 'react';
import * as TabsPrimitive from '@radix-ui/react-tabs';
import {
  segmentedItemSizeClass,
  segmentedTrackClass,
} from '@/components/ui/segmented-styles';
import { cn } from '@/lib/utils';

const Tabs = TabsPrimitive.Root;

const TabsList = React.forwardRef<
  React.ElementRef<typeof TabsPrimitive.List>,
  React.ComponentPropsWithoutRef<typeof TabsPrimitive.List>
>(({ className, ...props }, ref) => (
  <TabsPrimitive.List
    ref={ref}
    className={cn(segmentedTrackClass, className)}
    {...props}
  />
));
TabsList.displayName = 'TabsList';

const TabsTrigger = React.forwardRef<
  React.ElementRef<typeof TabsPrimitive.Trigger>,
  React.ComponentPropsWithoutRef<typeof TabsPrimitive.Trigger>
>(({ className, ...props }, ref) => (
  <TabsPrimitive.Trigger
    ref={ref}
    className={cn(
      // 页级导航：固定 md 档，与 AgentTabStrip / segmentedItemClass(md) 同高
      'inline-flex items-center justify-center rounded-btn transition-colors',
      segmentedItemSizeClass('md'),
      'text-secondary data-[state=inactive]:hover:bg-panel/50 data-[state=inactive]:hover:text-primary',
      'data-[state=active]:bg-panel data-[state=active]:font-medium data-[state=active]:text-primary data-[state=active]:shadow-sm',
      className,
    )}
    {...props}
  />
));
TabsTrigger.displayName = 'TabsTrigger';

/** 段距由 `pageRhythm.chrome` 承担，避免与页头底距叠一层。 */
const TabsContent = React.forwardRef<
  React.ElementRef<typeof TabsPrimitive.Content>,
  React.ComponentPropsWithoutRef<typeof TabsPrimitive.Content>
>(({ className, ...props }, ref) => (
  <TabsPrimitive.Content ref={ref} className={cn('focus:outline-none', className)} {...props} />
));
TabsContent.displayName = 'TabsContent';

export { Tabs, TabsList, TabsTrigger, TabsContent };
