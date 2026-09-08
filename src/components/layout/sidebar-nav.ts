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
import type { NavVisibility, OptionalNavId } from '@/lib/ui-preferences';
import { ROUTES_PATH, SUB2API_PATH } from '@/lib/routes-path';

export const PLUGINS_PATH = '/plugins';

/** 对话、Agent、总览、设置：侧栏始终显示，设置页不可关闭。 */
export const ALWAYS_VISIBLE_NAV_PATHS = ['/chat', '/agents', '/', '/settings'] as const;

/** 可开关入口的路径。缺省不在此表里的侧栏项始终显示。 */
export const OPTIONAL_NAV_PATH = {
  skills: '/skills',
  mcp: '/mcp',
  projects: '/projects',
  plugins: PLUGINS_PATH,
  connections: '/connections',
  sub2api: SUB2API_PATH,
  routes: ROUTES_PATH,
} as const satisfies Record<OptionalNavId, string>;

const OPTIONAL_NAV_ID_BY_PATH = new Map<string, OptionalNavId>(
  (Object.entries(OPTIONAL_NAV_PATH) as [OptionalNavId, string][]).map(([id, to]) => [to, id]),
);

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

export function optionalNavIdForPath(path: string): OptionalNavId | undefined {
  return OPTIONAL_NAV_ID_BY_PATH.get(path);
}

/** 按设置里的侧栏开关过滤；关闭后页面仍可通过 URL 打开。 */
export function filterNavItems<T extends { to: string }>(
  items: readonly T[],
  visibility: NavVisibility,
): T[] {
  return items.filter((item) => {
    const id = OPTIONAL_NAV_ID_BY_PATH.get(item.to);
    return id ? visibility[id] : true;
  });
}

/** 工作区条目经可选入口可见性过滤。顺序真源是 NAV_WORKSPACE。 */
export function workspaceNavItems(visibility: NavVisibility): SidebarNavItem[] {
  return filterNavItems(NAV_WORKSPACE, visibility);
}

/** 管理区条目经可选入口可见性过滤。顺序真源是 NAV_MANAGE。 */
export function manageNavItems(visibility: NavVisibility): SidebarNavItem[] {
  return filterNavItems(NAV_MANAGE, visibility);
}
