import * as React from 'react';
import { NavLink } from 'react-router-dom';
import { PanelLeftClose, PanelLeftOpen, Route } from 'lucide-react';
import { NavResizeHandle } from '@/components/layout/NavResizeHandle';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { ROUTES_NAV_WIDTH } from '@/components/layout/sidebar-width-model';
import { useNavWidth } from '@/components/layout/use-sidebar-width';
import { Badge } from '@/components/ui/badge';
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

const NAV_ICON_SIZE = 18;
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
  itemClass,
}: {
  item: RoutesNavItem;
  compact: boolean;
  itemClass: (isActive: boolean) => string;
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
      className="block rounded-btn focus:outline-none focus-visible:ring-1 focus-visible:ring-accent/30"
    >
      {({ isActive }) => {
        const node = (
          <span className={cn(itemClass(isActive), compact && 'justify-center px-0')}>
            <item.icon
              size={NAV_ICON_SIZE}
              strokeWidth={1.6}
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

  const itemClass = (isActive: boolean) =>
    cn(
      'group relative flex h-8 w-full items-center gap-2.5 rounded-btn px-2.5 text-body transition-colors duration-150',
      isActive
        ? 'bg-accent-subtle font-medium text-primary [&_svg]:text-accent before:absolute before:inset-y-1.5 before:left-0 before:w-0.5 before:rounded-full before:bg-accent'
        : 'text-secondary hover:bg-hover hover:text-primary',
    );

  const railToggleClass =
    'flex h-7 w-7 items-center justify-center rounded-btn text-muted transition-colors hover:bg-hover hover:text-primary focus:outline-none focus-visible:ring-1 focus-visible:ring-accent/30';

  return (
    <>
      <aside
        className={cn(pageRhythm.shellNav, 'relative', width.widthTransition)}
        style={{ width: width.width }}
        data-routes-nav
      >
      <div
        className={cn(
          'flex shrink-0 items-center border-b border-border',
          pageRhythm.topChrome,
          isLg && !collapsed ? 'justify-between px-3' : 'justify-center',
        )}
      >
        {isLg && !collapsed ? (
          <>
            <div className="flex min-w-0 items-center gap-2">
              <Route
                size={NAV_ICON_SIZE}
                strokeWidth={1.6}
                absoluteStrokeWidth
                data-icon="nav"
                className="shrink-0"
              />
              <span className={cn('min-w-0 truncate', pageRhythm.pageTitle)}>
                {t('routes.nav.title')}
              </span>
            </div>
            <Hint label={t('routes.nav.collapse')} side="right">
              <button
                type="button"
                onClick={() => setRailCollapsed(true)}
                className={railToggleClass}
                aria-label={t('routes.nav.collapse')}
              >
                <PanelLeftClose size={18} strokeWidth={1.6} absoluteStrokeWidth data-icon="nav" />
              </button>
            </Hint>
          </>
        ) : (
          <Hint label={t('routes.nav.expand')} side="right">
            <button
              type="button"
              onClick={() => setRailCollapsed(false)}
              className="group relative flex h-7 w-7 shrink-0 items-center justify-center rounded-btn focus:outline-none focus-visible:ring-1 focus-visible:ring-accent/30"
              aria-label={t('routes.nav.expand')}
            >
              <span className="flex h-7 w-7 items-center justify-center rounded-btn transition-opacity group-hover:opacity-0 group-focus-visible:opacity-0">
                <Route
                  size={NAV_ICON_SIZE}
                  strokeWidth={1.6}
                  absoluteStrokeWidth
                  data-icon="nav"
                />
              </span>
              <span className="absolute inset-0 flex items-center justify-center rounded-btn text-muted opacity-0 transition-opacity group-hover:bg-hover group-hover:text-primary group-hover:opacity-100 group-focus-visible:bg-hover group-focus-visible:text-primary group-focus-visible:opacity-100">
                <PanelLeftOpen size={18} strokeWidth={1.6} absoluteStrokeWidth data-icon="nav" />
              </span>
            </button>
          </Hint>
        )}
      </div>

      <nav
        aria-label={t('routes.nav.aria')}
        className={cn('flex min-h-0 flex-1 flex-col gap-0.5 pt-1 px-2')}
      >
        {navItems.map((item) => (
          <RoutesNavLink
            key={item.to}
            item={item}
            compact={compact}
            itemClass={itemClass}
          />
        ))}
      </nav>
      </aside>
      <NavResizeHandle
        label={t('routes.nav.resize')}
        width={width}
        interactive={isLg && !collapsed}
      />
    </>
  );
}
