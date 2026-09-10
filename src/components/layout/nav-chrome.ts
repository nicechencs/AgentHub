import { PanelLeftClose, PanelLeftOpen } from 'lucide-react';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { cn } from '@/lib/utils';
import { NAV } from '@/styles/tokens';

/** 18px rail icons — same stroke as `ICON.nav`. */
export const NAV_ICON_SIZE = NAV.icon.px;
export const NAV_ICON_STROKE = NAV.icon.stroke;

const MENU_ICON_CLASS = 'h-3.5 w-3.5';

/** 右键菜单图标：与折叠按钮同款 PanelLeft 图标 */
export const railMenuIcon = {
  expand: { className: MENU_ICON_CLASS, strokeWidth: 1.8 },
  collapse: { className: MENU_ICON_CLASS, strokeWidth: 1.8 },
} as const;

export const RailExpandIcon = PanelLeftOpen;
export const RailCollapseIcon = PanelLeftClose;

/** Focus ring on the NavLink / header control wrapper. */
export const navFocusClass =
  'rounded-btn focus:outline-none focus-visible:ring-1 focus-visible:ring-accent/30';

export const navListClass =
  'flex min-h-0 flex-1 flex-col gap-0.5 px-2 pt-1';

export const navToggleClass =
  'flex h-7 w-7 items-center justify-center rounded-btn text-muted transition-colors hover:bg-hover hover:text-primary focus:outline-none focus-visible:ring-1 focus-visible:ring-accent/30';

/** Brand / section title in an expanded rail header. Body serif, not a page title. */
export const navBrandTitleClass = 'truncate text-body font-semibold tracking-tight text-primary';

export function navHeaderClass(collapsed: boolean): string {
  return cn(
    'flex shrink-0 items-center border-b border-border',
    pageRhythm.topChrome,
    collapsed ? 'justify-center' : 'justify-between px-3',
  );
}

/**
 * One selected / hover language for expanded sidebar, icon rail, and Routes rail.
 * Settings page tabs stay a pill bar (`segmented-styles`); they share hover/focus,
 * not this accent-bar rail treatment.
 */
export function navItemClass(active: boolean, collapsed: boolean): string {
  return cn(
    'group relative flex h-8 w-full items-center rounded-btn text-body transition-colors duration-150',
    collapsed ? 'justify-center' : 'gap-2.5 px-2.5',
    active
      ? 'bg-accent-subtle font-medium text-primary [&_svg]:text-accent'
      : 'text-secondary hover:bg-hover hover:text-primary',
    active &&
      !collapsed &&
      'before:absolute before:inset-y-1.5 before:left-0 before:w-0.5 before:rounded-full before:bg-accent',
  );
}
