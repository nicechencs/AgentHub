/**
 * 最左侧栏自动折叠策略：只有点一级「路由」且设置打开时才收起。
 * 其它导航、刷新、路由区内切换、离开路由页都不改折叠状态。
 */

import { ROUTES_PATH } from '@/lib/routes-path';

export function collapsedAfterPrimaryNavClick(opts: {
  itemTo: string;
  currentCollapsed: boolean;
  autoCollapseOnRoutes: boolean;
}): boolean {
  if (opts.autoCollapseOnRoutes && opts.itemTo === ROUTES_PATH) return true;
  return opts.currentCollapsed;
}
