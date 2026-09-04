import {
  Activity,
  Cloud,
  KeyRound,
  LayoutDashboard,
  Network,
  type LucideIcon,
} from 'lucide-react';
import {
  ROUTES_ACTIVITY_PATH,
  ROUTES_BOARD_PATH,
  ROUTES_POOL_PATH,
  ROUTES_SUB2API_PATH,
  ROUTES_TOKENS_PATH,
  ROUTES_PATH,
} from '@/lib/routes-path';
import type { MessageKey } from '@/lib/i18n';

export type RoutesNavItem = {
  to: string;
  /** i18n key under routes.nav.* */
  labelKey: MessageKey;
  icon: LucideIcon;
  inDevelopment?: boolean;
};

export const ROUTES_NAV_ITEMS: readonly RoutesNavItem[] = [
  { to: ROUTES_BOARD_PATH, labelKey: 'routes.nav.board', icon: LayoutDashboard },
  { to: ROUTES_POOL_PATH, labelKey: 'routes.nav.pool', icon: Network },
  { to: ROUTES_TOKENS_PATH, labelKey: 'routes.nav.tokens', icon: KeyRound },
  { to: ROUTES_ACTIVITY_PATH, labelKey: 'routes.nav.activity', icon: Activity },
  { to: ROUTES_SUB2API_PATH, labelKey: 'routes.nav.sub2api', icon: Cloud },
] as const;

/** Filter Sub2API nav entry when the preference is off (deep link still works). */
export function visibleRoutesNavItems(sub2apiNavVisible: boolean): RoutesNavItem[] {
  if (sub2apiNavVisible) return [...ROUTES_NAV_ITEMS];
  return ROUTES_NAV_ITEMS.filter((item) => item.to !== ROUTES_SUB2API_PATH);
}

export function isRoutesAreaPath(pathname: string): boolean {
  return pathname === ROUTES_PATH || pathname.startsWith(`${ROUTES_PATH}/`);
}

export function routesNavItemInDevelopment(item: RoutesNavItem): boolean {
  return item.inDevelopment === true;
}
