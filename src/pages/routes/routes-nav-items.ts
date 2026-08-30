import {
  Activity,
  KeyRound,
  LayoutDashboard,
  List,
  Users,
  type LucideIcon,
} from 'lucide-react';
import { BRIDGES_PATH } from '@/lib/bridges-path';
import type { MessageKey } from '@/lib/i18n';

export type RoutesNavItem = {
  to: string;
  /** i18n key under routes.nav.* */
  labelKey: MessageKey;
  icon: LucideIcon;
  /** Exact match for index path so child routes do not light "列表". */
  end?: boolean;
  inDevelopment?: boolean;
};

export const ROUTES_NAV_ITEMS: readonly RoutesNavItem[] = [
  { to: BRIDGES_PATH, labelKey: 'routes.nav.list', icon: List, end: true },
  { to: `${BRIDGES_PATH}/board`, labelKey: 'routes.nav.board', icon: LayoutDashboard },
  {
    to: `${BRIDGES_PATH}/pool`,
    labelKey: 'routes.nav.pool',
    icon: Users,
    inDevelopment: true,
  },
  {
    to: `${BRIDGES_PATH}/tokens`,
    labelKey: 'routes.nav.tokens',
    icon: KeyRound,
    inDevelopment: true,
  },
  { to: `${BRIDGES_PATH}/activity`, labelKey: 'routes.nav.activity', icon: Activity },
] as const;

export function isRoutesAreaPath(pathname: string): boolean {
  return pathname === BRIDGES_PATH || pathname.startsWith(`${BRIDGES_PATH}/`);
}

export function routesNavItemInDevelopment(item: RoutesNavItem): boolean {
  return item.inDevelopment === true;
}
