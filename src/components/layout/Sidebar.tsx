import * as React from 'react';
import { NavLink, useLocation } from 'react-router-dom';
import { PanelLeftClose, PanelLeftOpen } from 'lucide-react';
import { AppLogo } from '@/components/shared/AppLogo';
import { StatusPin } from '@/components/shared/StatusPin';
import { useAppUpdateAvailable } from '@/app/runtime';
import { Hint } from '@/components/ui/tooltip';
import { collapsedAfterPrimaryNavClick } from '@/components/layout/sidebar-collapse-override';
import { NavResizeHandle } from '@/components/layout/NavResizeHandle';
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
const NAV_ICON_SIZE = 18;
const MENU_ICON_CLASS = 'h-3.5 w-3.5';

/** 右键菜单图标：与折叠按钮同款 PanelLeft 图标 */
const railMenuIcon = {
  expand: <PanelLeftOpen className={MENU_ICON_CLASS} strokeWidth={1.8} />,
  collapse: <PanelLeftClose className={MENU_ICON_CLASS} strokeWidth={1.8} />,
} as const;

function SidebarNavLink({
  item,
  collapsed,
  itemClass,
  notice,
}: {
  item: SidebarNavItem;
  collapsed: boolean;
  itemClass: (isActive: boolean) => string;
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
      className="block rounded-btn focus:outline-none focus-visible:ring-1 focus-visible:ring-accent/30"
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
          <span className={cn(itemClass(isActive), 'relative')}>
            <span className="relative shrink-0">
              <Icon
                size={NAV_ICON_SIZE}
                strokeWidth={1.6}
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
  const { collapsed, setCollapsed, toggle, navVisible } = useSidebar();
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

  const itemClass = (isActive: boolean) =>
    cn(
      'group relative flex h-8 w-full items-center rounded-btn text-body transition-colors duration-150',
      collapsed ? 'justify-center' : 'gap-2.5 px-2.5',
      isActive
        ? 'bg-accent-subtle font-medium text-primary [&_svg]:text-accent'
        : 'text-secondary hover:bg-hover hover:text-primary',
      isActive &&
        !collapsed &&
        'before:absolute before:inset-y-1.5 before:left-0 before:w-0.5 before:rounded-full before:bg-accent',
    );

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
        {/* 品牌 + 折叠按钮 */}
        <div
          className={cn(
            'flex shrink-0 items-center border-b border-border',
            pageRhythm.topChrome,
            collapsed ? 'justify-center' : 'justify-between px-3',
          )}
        >
          {collapsed ? (
            <Hint label={t('nav.expandSidebar')} side="right">
              <button
                type="button"
                onClick={toggle}
                className="group relative flex h-7 w-7 shrink-0 items-center justify-center rounded-btn focus:outline-none focus-visible:ring-1 focus-visible:ring-accent/30"
                aria-label={t('nav.expandSidebar')}
              >
                <span className="flex h-7 w-7 items-center justify-center rounded-btn transition-opacity group-hover:opacity-0 group-focus-visible:opacity-0">
                  <AppLogo size={20} className="h-5 w-5" />
                </span>
                <span className="absolute inset-0 flex items-center justify-center rounded-btn text-muted opacity-0 transition-opacity group-hover:bg-hover group-hover:text-primary group-hover:opacity-100 group-focus-visible:bg-hover group-focus-visible:text-primary group-focus-visible:opacity-100">
                  <PanelLeftOpen size={18} strokeWidth={1.6} absoluteStrokeWidth data-icon="nav" />
                </span>
              </button>
            </Hint>
          ) : (
            <>
              <div className="flex min-w-0 items-center gap-2">
                <span className="flex h-6 w-6 shrink-0 items-center justify-center rounded-btn">
                  <AppLogo size={20} className="h-5 w-5" />
                </span>
                <span className="truncate text-sm font-semibold tracking-tight">AgentHub</span>
              </div>
              <Hint label={t('nav.collapseSidebar')} side="right">
                <button
                  type="button"
                  onClick={toggle}
                  className="flex h-7 w-7 items-center justify-center rounded-btn text-muted transition-colors hover:bg-hover hover:text-primary"
                  aria-label={t('nav.collapseSidebar')}
                >
                  <PanelLeftClose size={18} strokeWidth={1.6} absoluteStrokeWidth data-icon="nav" />
                </button>
              </Hint>
            </>
          )}
        </div>

        {/* 工作区置顶；管理区 mt-auto 贴底 */}
        <nav
          className="flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto overscroll-contain px-2 pt-1"
        >
          <NavGroup label={t('nav.workspace')} collapsed={collapsed}>
            {visibleWorkspaceNav.map((item) => (
              <SidebarNavLink key={item.to} item={item} collapsed={collapsed} itemClass={itemClass} />
            ))}
          </NavGroup>
          <NavGroup label={t('nav.manage')} collapsed={collapsed} className="mt-auto pb-2">
            {visibleManageNav.map((item) => (
              <SidebarNavLink
                key={item.to}
                item={item}
                collapsed={collapsed}
                itemClass={itemClass}
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
            {railMenuIcon.expand}
            {t('nav.expandSidebar')}
          </ContextMenuItem>
        ) : (
          <ContextMenuItem onSelect={collapseFromRailMenu}>
            {railMenuIcon.collapse}
            {t('nav.collapseSidebar')}
          </ContextMenuItem>
        )}
      </ContextMenu>
    </>
  );
}
