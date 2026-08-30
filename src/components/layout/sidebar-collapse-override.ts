/**
 * 路由区会话级一级侧栏折叠覆盖：不写 localStorage。
 * stored = 用户持久偏好；session = null 表示无覆盖。
 */

export type SidebarCollapseMode = {
  stored: boolean;
  session: boolean | null;
};

export function effectiveCollapsed(mode: SidebarCollapseMode): boolean {
  return mode.session ?? mode.stored;
}

/** 进入 `/routes*`：若尚无会话覆盖，自动折叠一级侧栏。 */
export function onEnterRoutesArea(mode: SidebarCollapseMode): SidebarCollapseMode {
  if (mode.session !== null) return mode;
  return { ...mode, session: true };
}

/** 二级导航「展开侧栏」：会话内展开（即使持久偏好是折叠）。 */
export function onExpandPrimaryFromRoutes(mode: SidebarCollapseMode): SidebarCollapseMode {
  return { ...mode, session: false };
}

/** 路由区内点一级侧栏折叠钮：只改会话覆盖，不落盘。 */
export function onToggleInRoutesArea(mode: SidebarCollapseMode): SidebarCollapseMode {
  return { ...mode, session: !effectiveCollapsed(mode) };
}

/** 离开路由区：清除覆盖，恢复持久偏好。 */
export function onLeaveRoutesArea(mode: SidebarCollapseMode): SidebarCollapseMode {
  return { ...mode, session: null };
}
