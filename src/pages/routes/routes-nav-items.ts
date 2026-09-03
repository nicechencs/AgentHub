import {
  Activity,
  KeyRound,
  LayoutDashboard,
  Network,
  type LucideIcon,
} from 'lucide-react';
import {
  ROUTES_ACTIVITY_PATH,
  ROUTES_BOARD_PATH,
  ROUTES_POOL_PATH,
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
] as const;

export function isRoutesAreaPath(pathname: string): boolean {
  return pathname === ROUTES_PATH || pathname.startsWith(`${ROUTES_PATH}/`);
}

export function routesNavItemInDevelopment(item: RoutesNavItem): boolean {
  return item.inDevelopment === true;
}
