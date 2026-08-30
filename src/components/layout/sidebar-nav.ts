import {
  Blocks,
  Bot,
  Cable,
  FolderKanban,
  Gauge,
  Key,
  MessagesSquare,
  Plug,
  Puzzle,
  Settings2,
} from 'lucide-react';
import { BRIDGES_PATH } from '@/lib/bridges-path';

export const PLUGINS_PATH = '/plugins';

/** 工作区 */
export const NAV_WORKSPACE = [
  { to: '/chat', navKey: 'nav.chat', icon: MessagesSquare },
  { to: '/agents', navKey: 'nav.agents', icon: Bot },
  { to: '/skills', navKey: 'nav.skills', icon: Blocks },
  { to: '/mcp', navKey: 'nav.mcp', icon: Plug, inDevelopment: true },
  { to: '/projects', navKey: 'nav.projects', icon: FolderKanban },
  { to: '/plugins', navKey: 'nav.plugins', icon: Puzzle, inDevelopment: true },
] as const;

/** 管理 */
export const NAV_MANAGE = [
  { to: '/', navKey: 'nav.dashboard', icon: Gauge },
  { to: '/connections', navKey: 'nav.connections', icon: Key },
  { to: BRIDGES_PATH, navKey: 'nav.routes', icon: Cable, inDevelopment: true },
  { to: '/settings', navKey: 'nav.settings', icon: Settings2 },
] as const;

export type SidebarNavItem = (typeof NAV_WORKSPACE)[number] | (typeof NAV_MANAGE)[number];

export function navItemInDevelopment(item: SidebarNavItem): boolean {
  return 'inDevelopment' in item && item.inDevelopment === true;
}

/** 管理区导航：按「路由」可见性过滤（路由仍可通过 URL 直接访问）。 */
export function filterManageNavItems<T extends { to: string }>(
  items: readonly T[],
  routesNavVisible: boolean,
): T[] {
  if (routesNavVisible) return [...items];
  return items.filter((item) => item.to !== BRIDGES_PATH);
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

/** 管理区条目经路由入口可见性过滤。顺序真源是 NAV_MANAGE。 */
export function manageNavItems(routesNavVisible: boolean): SidebarNavItem[] {
  return filterManageNavItems(NAV_MANAGE, routesNavVisible);
}
