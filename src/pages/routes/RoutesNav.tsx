import * as React from 'react';
import { NavLink } from 'react-router-dom';
import { Route } from 'lucide-react';
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
import { pageRhythm } from '@/components/layout/page-rhythm';
import { ROUTES_NAV_WIDTH } from '@/components/layout/sidebar-width-model';
import { useNavWidth } from '@/components/layout/use-sidebar-width';
import { Badge } from '@/components/ui/badge';
import {
  ContextMenu,
  ContextMenuItem,
  type ContextMenuPoint,
} from '@/components/ui/context-menu';
import { Hint } from '@/components/ui/tooltip';
import { useI18n } from '@/components/shared/LanguageProvider';
import { StorageKey } from '@/lib/storage-key';
import { loadBool, saveBool } from '@/lib/ui-preferences';
import { cn } from '@/lib/utils';
import {
  ROUTES_NAV_ITEMS,
  routesNavItemInDevelopment,
  type RoutesNavItem,
} from '@/pages/routes/routes-nav-items';

const LG_QUERY = '(min-width: 1024px)';

function useIsLgUp() {
  const [isLg, setIsLg] = React.useState(() =>
    typeof window !== 'undefined' ? window.matchMedia(LG_QUERY).matches : true,
  );
  React.useEffect(() => {
    const mq = window.matchMedia(LG_QUERY);
    const onChange = () => setIsLg(mq.matches);
    onChange();
    mq.addEventListener('change', onChange);
    return () => mq.removeEventListener('change', onChange);
  }, []);
  return isLg;
}

function RoutesNavLink({
  item,
  compact,
}: {
  item: RoutesNavItem;
  compact: boolean;
}) {
  const { t } = useI18n();
  const label = t(item.labelKey);
  const inDevelopment = routesNavItemInDevelopment(item);
  const developmentLabel = t('common.inDevelopment');
  const a11yLabel = [label, inDevelopment ? developmentLabel : null].filter(Boolean).join(' — ');

  return (
    <NavLink
      to={item.to}
      aria-label={compact || inDevelopment ? a11yLabel : undefined}
      className={cn('block', navFocusClass)}
    >
      {({ isActive }) => {
        const node = (
          <span className={navItemClass(isActive, compact)}>
            <item.icon
              size={NAV_ICON_SIZE}
              strokeWidth={NAV_ICON_STROKE}
              absoluteStrokeWidth
              data-icon="nav"
              className="shrink-0"
            />
            {!compact && (
              <>
                <span className="truncate">{label}</span>
                {inDevelopment && (
                  <Badge variant="default" className="ml-auto shrink-0" aria-hidden>
                    {developmentLabel}
                  </Badge>
                )}
              </>
            )}
          </span>
        );
        if (!compact) return node;
        return (
          <Hint label={a11yLabel} side="right">
            {node}
          </Hint>
        );
      }}
    </NavLink>
  );
}

/**
 * 路由区二级导航：shell 级第三块圆角面板。
 * 由 App 在 `/routes*` 时条件渲染，与一级侧栏并列。
 */
export function RoutesNav() {
  const { t } = useI18n();
  const isLg = useIsLgUp();
  const [collapsed, setCollapsed] = React.useState(
    () => loadBool(StorageKey.routesNavCollapsed, false),
  );
  const compact = !isLg || collapsed;
  const width = useNavWidth({
    collapsed: compact,
    storageKey: StorageKey.routesNavWidth,
    policy: ROUTES_NAV_WIDTH,
  });
  const navItems = ROUTES_NAV_ITEMS;

  const setRailCollapsed = React.useCallback((next: boolean) => {
    setCollapsed(next);
    saveBool(StorageKey.routesNavCollapsed, next);
  }, []);

  const [railMenu, setRailMenu] = React.useState<ContextMenuPoint | null>(null);
  const openRailMenu = (e: React.MouseEvent) => {
    e.preventDefault();
    setRailMenu({ x: e.clientX, y: e.clientY });
  };
  const closeRailMenu = React.useCallback(() => setRailMenu(null), []);
  const expandFromRailMenu = React.useCallback(() => {
    setRailCollapsed(false);
    setRailMenu(null);
  }, [setRailCollapsed]);
  const collapseFromRailMenu = React.useCallback(() => {
    setRailCollapsed(true);
    setRailMenu(null);
  }, [setRailCollapsed]);

  return (
    <>
      <aside
        className={cn(pageRhythm.shellNav, 'relative', width.widthTransition)}
        style={{ width: width.width }}
        data-routes-nav
        onContextMenu={openRailMenu}
      >
        <NavRailHeader
          collapsed={compact}
          title={t('routes.nav.title')}
          expandLabel={t('routes.nav.expand')}
          collapseLabel={t('routes.nav.collapse')}
          onExpand={() => setRailCollapsed(false)}
          onCollapse={() => setRailCollapsed(true)}
          mark={
            <Route
              size={NAV_ICON_SIZE}
              strokeWidth={NAV_ICON_STROKE}
              absoluteStrokeWidth
              data-icon="nav"
              className="shrink-0"
            />
          }
        />

        <nav aria-label={t('routes.nav.aria')} className={navListClass}>
          {navItems.map((item) => (
            <RoutesNavLink key={item.to} item={item} compact={compact} />
          ))}
        </nav>
      </aside>
      <NavResizeHandle
        label={t('routes.nav.resize')}
        width={width}
        interactive={isLg && !collapsed}
      />
      <ContextMenu open={railMenu !== null} point={railMenu} onClose={closeRailMenu}>
        {collapsed ? (
          <ContextMenuItem onSelect={expandFromRailMenu}>
            <RailExpandIcon className={railMenuIcon.expand.className} strokeWidth={railMenuIcon.expand.strokeWidth} />
            {t('routes.nav.expand')}
          </ContextMenuItem>
        ) : (
          <ContextMenuItem onSelect={collapseFromRailMenu}>
            <RailCollapseIcon className={railMenuIcon.collapse.className} strokeWidth={railMenuIcon.collapse.strokeWidth} />
            {t('routes.nav.collapse')}
          </ContextMenuItem>
        )}
      </ContextMenu>
    </>
  );
}
