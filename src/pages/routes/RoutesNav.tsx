import * as React from 'react';
import { NavLink } from 'react-router-dom';
import { NavResizeHandle } from '@/components/layout/NavResizeHandle';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { ROUTES_NAV_WIDTH } from '@/components/layout/sidebar-width-model';
import { useNavWidth } from '@/components/layout/use-sidebar-width';
import { Badge } from '@/components/ui/badge';
import { Hint } from '@/components/ui/tooltip';
import { useI18n } from '@/components/shared/LanguageProvider';
import { StorageKey } from '@/lib/storage-key';
import { cn } from '@/lib/utils';
import {
  ROUTES_NAV_ITEMS,
  routesNavItemInDevelopment,
  type RoutesNavItem,
} from '@/pages/routes/routes-nav-items';

const NAV_ICON_SIZE = 22;
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
      className="block focus:outline-none focus-visible:ring-1 focus-visible:ring-accent/30"
    >
      {({ isActive }) => {
        const node = (
          <span className={itemClass(isActive)}>
            <span className={pageRhythm.railSlot}>
              <item.icon
                size={NAV_ICON_SIZE}
                strokeWidth={1.6}
                absoluteStrokeWidth
                data-icon="nav"
                className="shrink-0"
              />
            </span>
            {!compact && (
              <>
                <span className="min-w-0 flex-1 truncate pr-2">{label}</span>
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
 * 路由区二级导航：与一级侧栏并列的贴边 chrome。
 * 由 App 在 `/routes*` 时条件渲染，与一级侧栏并列。
 */
export function RoutesNav() {
  const { t } = useI18n();
  const isLg = useIsLgUp();
  const width = useNavWidth({
    collapsed: !isLg,
    storageKey: StorageKey.routesNavWidth,
    policy: ROUTES_NAV_WIDTH,
  });
  const navItems = ROUTES_NAV_ITEMS;

  const itemClass = (isActive: boolean) =>
    cn(
      'group relative flex h-12 w-full items-center text-body transition-colors duration-150',
      isActive
        ? 'font-medium text-primary [&_svg]:text-accent before:absolute before:inset-y-2.5 before:left-0 before:w-0.5 before:rounded-full before:bg-accent'
        : 'text-secondary hover:bg-hover hover:text-primary',
      isActive && isLg && 'bg-accent-subtle',
    );

  return (
    <aside
      className={cn(pageRhythm.shellNav, 'relative', width.widthTransition)}
      style={{ width: width.width }}
      data-routes-nav
    >
      <nav
        aria-label={t('routes.nav.aria')}
        className="flex min-h-0 flex-1 flex-col overflow-y-auto overscroll-contain"
      >
        {navItems.map((item) => (
          <RoutesNavLink
            key={item.to}
            item={item}
            compact={!isLg}
            itemClass={itemClass}
          />
        ))}
      </nav>
      {isLg && <NavResizeHandle label={t('routes.nav.resize')} width={width} />}
    </aside>
  );
}
