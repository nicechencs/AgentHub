import * as React from 'react';
import { NavLink, useLocation } from 'react-router-dom';
import { AppLogo } from '@/components/shared/AppLogo';
import { StatusPin } from '@/components/shared/StatusPin';
import { useAppUpdateAvailable } from '@/app/runtime';
import { Hint } from '@/components/ui/tooltip';
import { collapsedAfterPrimaryNavClick } from '@/components/layout/sidebar-collapse-override';
import { NavRailHeader } from '@/components/layout/NavRailHeader';
import { NavResizeHandle } from '@/components/layout/NavResizeHandle';
import {
  NAV_ICON_SIZE,
  NAV_ICON_STROKE,
  RailCollapseIcon,
  RailExpandIcon,
  navFocusClass,
  navItemClass,
  navListClass,
  railMenuIcon,
} from '@/components/layout/nav-chrome';
import { useSidebar } from '@/components/layout/SidebarContext';
import { useSidebarWidth } from '@/components/layout/use-sidebar-width';
import {
  manageNavItems,
  navItemInDevelopment,
  type SidebarNavItem,
  workspaceNavItems,
} from '@/components/layout/sidebar-nav';
import { Badge } from '@/components/ui/badge';
import { pageRhythm } from '@/components/layout/page-rhythm';
import {
  ContextMenu,
  ContextMenuItem,
  type ContextMenuPoint,
} from '@/components/ui/context-menu';
import { cn } from '@/lib/utils';
import { useI18n } from '@/components/shared/LanguageProvider';
import { isRoutesAreaPath } from '@/pages/routes/routes-nav-items';

function SidebarNavLink({
  item,
  collapsed,
  notice,
}: {
  item: SidebarNavItem;
  collapsed: boolean;
  /** Optional silent tip (e.g. app update available on Settings). */
  notice?: { label: string } | null;
}) {
  const { t } = useI18n();
  const { setCollapsed, autoCollapseOnRoutes } = useSidebar();
  const { to, navKey, icon: Icon } = item;
  const label = t(navKey);
  const inDevelopment = navItemInDevelopment(item);
  const developmentLabel = t('common.inDevelopment');
  const tip = notice?.label;
  const a11yLabel = [label, inDevelopment ? developmentLabel : null, tip]
    .filter(Boolean)
    .join(' — ');

  return (
    <NavLink
      to={to}
      end={to === '/'}
      aria-label={collapsed || tip || inDevelopment ? a11yLabel : undefined}
      className={cn('block', navFocusClass)}
      onClick={() => {
        const next = collapsedAfterPrimaryNavClick({
          itemTo: to,
          currentCollapsed: collapsed,
          autoCollapseOnRoutes,
        });
        if (next !== collapsed) setCollapsed(next);
      }}
    >
      {({ isActive }) => {
        const node = (
          <span className={cn(navItemClass(isActive, collapsed), 'relative')}>
            <span className="relative shrink-0">
              <Icon
                size={NAV_ICON_SIZE}
                strokeWidth={NAV_ICON_STROKE}
                absoluteStrokeWidth
                data-icon="nav"
                className="shrink-0"
              />
              {/* Collapsed: corner pin on icon only (expanded uses trailing pin). */}
              {notice && collapsed && <StatusPin tone="warning" ring="panel" corner />}
            </span>
            {!collapsed && (
              <>
                <span className="truncate">{label}</span>
                {inDevelopment && (
                  <Badge variant="default" className="ml-auto shrink-0" aria-hidden>
                    {developmentLabel}
                  </Badge>
                )}
                {notice && <StatusPin tone="warning" label={tip} className="ml-auto" />}
              </>
            )}
          </span>
        );

        if (!collapsed) {
          if (!tip) return node;
          return (
            <Hint label={tip} side="right">
              {node}
            </Hint>
          );
        }

        return (
          <Hint label={a11yLabel} side="right">
            {node}
          </Hint>
        );
      }}
    </NavLink>
  );
}

function NavGroup({
  label,
  collapsed,
  className,
  children,
}: {
  label: string;
  collapsed: boolean;
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <div className={cn('flex shrink-0 flex-col gap-0.5', className)}>
      {!collapsed && (
        <div className={cn('px-2.5 pb-1 pt-2', pageRhythm.sectionEyebrow)}>
          {label}
        </div>
      )}
      {collapsed && <div className="h-2" aria-hidden />}
      {children}
    </div>
  );
}

/** 侧边导航：可折叠 */
export function Sidebar() {
  const { collapsed, setCollapsed, navVisible } = useSidebar();
  const width = useSidebarWidth(collapsed);
  const { pathname } = useLocation();
  const { t } = useI18n();
  const appUpdate = useAppUpdateAvailable();
  const settingsNotice = appUpdate
    ? { label: t('nav.updateAvailable', { version: appUpdate.version }) }
    : null;

  // 右键导航栏：展开态只允许收起，收起态只允许展开
  const [railMenu, setRailMenu] = React.useState<ContextMenuPoint | null>(null);
  const openRailMenu = (e: React.MouseEvent) => {
    e.preventDefault();
    setRailMenu({ x: e.clientX, y: e.clientY });
  };
  const closeRailMenu = React.useCallback(() => setRailMenu(null), []);
  const expandFromRailMenu = React.useCallback(() => {
    setCollapsed(false);
    setRailMenu(null);
  }, [setCollapsed]);
  const collapseFromRailMenu = React.useCallback(() => {
    setCollapsed(true);
    setRailMenu(null);
  }, [setCollapsed]);

  const visibleWorkspaceNav = React.useMemo(
    () => workspaceNavItems(navVisible),
    [navVisible],
  );
  // Deep-link into /routes* still shows the Routes entry so the primary nav
  // has an active item; preference remains off when leaving the area.
  // Other optional pages stay preference-gated (deep link still opens the page).
  const visibleManageNav = React.useMemo(
    () =>
      manageNavItems({
        ...navVisible,
        routes: navVisible.routes || isRoutesAreaPath(pathname),
      }),
    [pathname, navVisible],
  );

  return (
    <>
      <aside
        className={cn(pageRhythm.shellNav, 'relative', width.widthTransition)}
        style={{ width: width.width }}
        onContextMenu={openRailMenu}
      >
        <NavRailHeader
          collapsed={collapsed}
          title="AgentHub"
          expandLabel={t('nav.expandSidebar')}
          collapseLabel={t('nav.collapseSidebar')}
          onExpand={() => setCollapsed(false)}
          onCollapse={() => setCollapsed(true)}
          mark={<AppLogo size={20} className="h-5 w-5" />}
        />

        {/* 工作区置顶；管理区 mt-auto 贴底 */}
        <nav className={cn(navListClass, 'overflow-y-auto overscroll-contain')}>
          <NavGroup label={t('nav.workspace')} collapsed={collapsed}>
            {visibleWorkspaceNav.map((item) => (
              <SidebarNavLink key={item.to} item={item} collapsed={collapsed} />
            ))}
          </NavGroup>
          <NavGroup label={t('nav.manage')} collapsed={collapsed} className="mt-auto pb-2">
            {visibleManageNav.map((item) => (
              <SidebarNavLink
                key={item.to}
                item={item}
                collapsed={collapsed}
                notice={item.to === '/settings' ? settingsNotice : null}
              />
            ))}
          </NavGroup>
        </nav>
      </aside>
      <NavResizeHandle
        label={t('nav.resizeSidebar')}
        width={width}
        interactive={!collapsed}
      />
      <ContextMenu open={railMenu !== null} point={railMenu} onClose={closeRailMenu}>
        {collapsed ? (
          <ContextMenuItem onSelect={expandFromRailMenu}>
            <RailExpandIcon className={railMenuIcon.expand.className} strokeWidth={railMenuIcon.expand.strokeWidth} />
            {t('nav.expandSidebar')}
          </ContextMenuItem>
        ) : (
          <ContextMenuItem onSelect={collapseFromRailMenu}>
            <RailCollapseIcon className={railMenuIcon.collapse.className} strokeWidth={railMenuIcon.collapse.strokeWidth} />
            {t('nav.collapseSidebar')}
          </ContextMenuItem>
        )}
      </ContextMenu>
    </>
  );
}
