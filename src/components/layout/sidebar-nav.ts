import { BRIDGES_PATH } from '@/lib/bridges-path';

export const PLUGINS_PATH = '/plugins';

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
