import { pageRhythm } from '@/components/layout/page-rhythm';
import type { NavWidthController } from '@/components/layout/use-sidebar-width';
import { cn } from '@/lib/utils';

export function NavResizeHandle({
  label,
  width,
  interactive = true,
}: {
  label: string;
  width: NavWidthController;
  /** Collapsed rails keep the centered rule, but are not draggable. */
  interactive?: boolean;
}) {
  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      aria-valuenow={width.paneWidth}
      aria-valuemin={width.valuemin}
      aria-valuemax={width.valuemax}
      tabIndex={interactive ? 0 : -1}
      onPointerDown={interactive ? width.onResizeStart : undefined}
      onDoubleClick={interactive ? width.resetWidth : undefined}
      onKeyDown={interactive ? width.onSeparatorKeyDown : undefined}
      className={cn(
        pageRhythm.sash,
        'absolute inset-y-0 right-0',
        !interactive && 'pointer-events-none',
      )}
    />
  );
}
