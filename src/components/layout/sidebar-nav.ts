import {
  Blocks,
  Bot,
  Cloud,
  FolderCode,
  Gauge,
  Key,
  MessageSquare,
  Plug,
  Puzzle,
  Route,
  Settings2,
} from 'lucide-react';
import { ROUTES_PATH, SUB2API_PATH } from '@/lib/routes-path';

export const PLUGINS_PATH = '/plugins';

/** 工作区 */
export const NAV_WORKSPACE = [
  { to: '/chat', navKey: 'nav.chat', icon: MessageSquare },
  { to: '/agents', navKey: 'nav.agents', icon: Bot },
  { to: '/skills', navKey: 'nav.skills', icon: Blocks },
  { to: '/mcp', navKey: 'nav.mcp', icon: Plug },
  { to: '/projects', navKey: 'nav.projects', icon: FolderCode },
  { to: '/plugins', navKey: 'nav.plugins', icon: Puzzle, inDevelopment: true },
] as const;

/** 管理 */
export const NAV_MANAGE = [
  { to: '/', navKey: 'nav.dashboard', icon: Gauge },
  { to: '/connections', navKey: 'nav.connections', icon: Key },
  { to: SUB2API_PATH, navKey: 'nav.sub2api', icon: Cloud },
  { to: ROUTES_PATH, navKey: 'nav.routes', icon: Route },
  { to: '/settings', navKey: 'nav.settings', icon: Settings2 },
] as const;

export type SidebarNavItem = (typeof NAV_WORKSPACE)[number] | (typeof NAV_MANAGE)[number];

export function navItemInDevelopment(item: SidebarNavItem): boolean {
  return 'inDevelopment' in item && item.inDevelopment === true;
}

/** 管理区导航：按「路由 / Sub2API」可见性过滤（页面仍可通过 URL 直接访问）。 */
export function filterManageNavItems<T extends { to: string }>(
  items: readonly T[],
  routesNavVisible: boolean,
  sub2apiNavVisible = true,
): T[] {
  return items.filter((item) => {
    if (!routesNavVisible && item.to === ROUTES_PATH) return false;
    if (!sub2apiNavVisible && item.to === SUB2API_PATH) return false;
    return true;
  });
}

/** 工作区导航：按「插件」可见性过滤（插件页仍可通过 URL 直接访问）。 */
export function filterWorkspaceNavItems<T extends { to: string }>(
  items: readonly T[],
  pluginsNavVisible: boolean,
): T[] {
  if (pluginsNavVisible) return [...items];
  return items.filter((item) => item.to !== PLUGINS_PATH);
}

/** 工作区条目经插件入口可见性过滤。顺序真源是 NAV_WORKSPACE。 */
export function workspaceNavItems(pluginsNavVisible: boolean): SidebarNavItem[] {
  return filterWorkspaceNavItems(NAV_WORKSPACE, pluginsNavVisible);
}

/** 管理区条目经路由 / Sub2API 入口可见性过滤。顺序真源是 NAV_MANAGE。 */
export function manageNavItems(
  routesNavVisible: boolean,
  sub2apiNavVisible: boolean,
): SidebarNavItem[] {
  return filterManageNavItems(NAV_MANAGE, routesNavVisible, sub2apiNavVisible);
}
