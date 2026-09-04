import * as React from 'react';
import { NavLink } from 'react-router-dom';
import { PanelLeftOpen } from 'lucide-react';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { useSidebar } from '@/components/layout/SidebarContext';
import { Badge } from '@/components/ui/badge';
import { Hint } from '@/components/ui/tooltip';
import { useI18n } from '@/components/shared/LanguageProvider';
import { cn } from '@/lib/utils';
import {
  routesNavItemInDevelopment,
  visibleRoutesNavItems,
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
  const { expandPrimarySidebar, sub2apiNavVisible } = useSidebar();
  const isLg = useIsLgUp();
  const navItems = React.useMemo(
    () => visibleRoutesNavItems(sub2apiNavVisible),
    [sub2apiNavVisible],
  );

  const itemClass = (isActive: boolean) =>
    cn(
      'group flex h-8 w-full items-center gap-2.5 rounded-btn px-2.5 text-sm transition-colors duration-150',
      isActive
        ? 'bg-active font-medium text-primary [&_svg]:text-accent'
        : 'text-secondary hover:bg-hover hover:text-primary',
    );

  return (
    <aside
      className={cn(
        pageRhythm.shellNav,
        'w-12 lg:w-48',
        'transition-[width] duration-200 ease-in-out motion-reduce:transition-none',
      )}
      data-routes-nav
    >
      <div
        className={cn(
          'flex shrink-0 items-center border-b border-border',
          pageRhythm.topChrome,
          isLg ? 'justify-between px-3' : 'justify-center',
        )}
      >
        <Hint label={t('nav.expandSidebar')} side="right">
          <button
            type="button"
            onClick={expandPrimarySidebar}
            className="flex h-7 w-7 items-center justify-center rounded-btn text-muted transition-colors hover:bg-hover hover:text-primary focus:outline-none focus-visible:ring-1 focus-visible:ring-accent/30"
            aria-label={t('nav.expandSidebar')}
          >
            <PanelLeftOpen size={18} strokeWidth={1.6} absoluteStrokeWidth />
          </button>
        </Hint>
        {isLg && (
          <span className="min-w-0 truncate text-sm font-semibold tracking-tight">
            {t('routes.nav.title')}
          </span>
        )}
      </div>

      <nav
        aria-label={t('routes.nav.aria')}
        className={cn('flex min-h-0 flex-1 flex-col gap-0.5 pt-1', isLg ? 'px-2' : 'px-1.5')}
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
    </aside>
  );
}
