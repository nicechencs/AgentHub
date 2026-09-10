import type { ReactNode } from 'react';
import { AppLogo } from '@/components/shared/AppLogo';
import { Hint } from '@/components/ui/tooltip';
import {
  NAV_ICON_SIZE,
  NAV_ICON_STROKE,
  RailCollapseIcon,
  RailExpandIcon,
  navBrandTitleClass,
  navFocusClass,
  navHeaderClass,
  navToggleClass,
} from '@/components/layout/nav-chrome';
import { cn } from '@/lib/utils';

/**
 * Shared collapse chrome for the primary sidebar and Routes secondary rail.
 * Expanded: mark + title + collapse. Collapsed: mark morphs to expand on hover.
 */
export function NavRailHeader({
  collapsed,
  title,
  expandLabel,
  collapseLabel,
  onExpand,
  onCollapse,
  mark,
}: {
  collapsed: boolean;
  title: string;
  expandLabel: string;
  collapseLabel: string;
  onExpand: () => void;
  onCollapse: () => void;
  /** Expanded-rail identity mark. Defaults to the product logo. */
  mark?: ReactNode;
}) {
  const identity = mark ?? <AppLogo size={20} className="h-5 w-5" />;

  if (collapsed) {
    return (
      <div className={navHeaderClass(true)}>
        <Hint label={expandLabel} side="right">
          <button
            type="button"
            onClick={onExpand}
            className={cn(
              'group relative flex h-7 w-7 shrink-0 items-center justify-center',
              navFocusClass,
            )}
            aria-label={expandLabel}
          >
            <span className="flex h-7 w-7 items-center justify-center rounded-btn transition-opacity group-hover:opacity-0 group-focus-visible:opacity-0">
              {identity}
            </span>
            <span className="absolute inset-0 flex items-center justify-center rounded-btn text-muted opacity-0 transition-opacity group-hover:bg-hover group-hover:text-primary group-hover:opacity-100 group-focus-visible:bg-hover group-focus-visible:text-primary group-focus-visible:opacity-100">
              <RailExpandIcon
                size={NAV_ICON_SIZE}
                strokeWidth={NAV_ICON_STROKE}
                absoluteStrokeWidth
                data-icon="nav"
              />
            </span>
          </button>
        </Hint>
      </div>
    );
  }

  return (
    <div className={navHeaderClass(false)}>
      <div className="flex min-w-0 items-center gap-2">
        <span className="flex h-6 w-6 shrink-0 items-center justify-center rounded-btn">
          {identity}
        </span>
        <span className={navBrandTitleClass}>{title}</span>
      </div>
      <Hint label={collapseLabel} side="right">
        <button
          type="button"
          onClick={onCollapse}
          className={navToggleClass}
          aria-label={collapseLabel}
        >
          <RailCollapseIcon
            size={NAV_ICON_SIZE}
            strokeWidth={NAV_ICON_STROKE}
            absoluteStrokeWidth
            data-icon="nav"
          />
        </button>
      </Hint>
    </div>
  );
}
