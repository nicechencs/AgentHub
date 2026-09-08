import { pageRhythm } from '@/components/layout/page-rhythm';
import type { NavWidthController } from '@/components/layout/use-sidebar-width';
import { cn } from '@/lib/utils';

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
      className={cn(pageRhythm.sash, 'absolute inset-y-0 right-0')}
    />
  );
}
