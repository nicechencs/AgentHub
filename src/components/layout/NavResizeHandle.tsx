import { cn } from '@/lib/utils';
import type { NavWidthController } from '@/components/layout/use-sidebar-width';

export function NavResizeHandle({
  label,
  width,
}: {
  label: string;
  width: NavWidthController;
}) {
  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      aria-valuenow={width.paneWidth}
      aria-valuemin={width.valuemin}
      aria-valuemax={width.valuemax}
      tabIndex={0}
      onPointerDown={width.onResizeStart}
      onDoubleClick={width.resetWidth}
      onKeyDown={width.onSeparatorKeyDown}
      className={cn(
        'absolute inset-y-0 right-0 z-10 w-1.5 cursor-col-resize touch-none bg-transparent outline-none',
        'hover:bg-accent/40 focus-visible:bg-accent/40 active:bg-accent/60',
        'before:absolute before:inset-y-0 before:-left-1 before:right-0 before:content-[""]',
      )}
    />
  );
}
