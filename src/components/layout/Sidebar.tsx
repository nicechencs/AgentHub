import * as React from 'react';
import { NavLink } from 'react-router-dom';
import { PanelLeftClose, PanelLeftOpen } from 'lucide-react';
import { AgentDot } from '@/components/shared/AgentDot';
import { AppLogo } from '@/components/shared/AppLogo';
import { StatusPin } from '@/components/shared/StatusPin';
import { AGENTS, type AgentMeta } from '@/config/agents';
import {
  useAgentStatusesOptional,
  useAppUpdateAvailable,
} from '@/app/runtime';
import type { AgentStatus } from '@/lib/types';
import { Hint } from '@/components/ui/tooltip';
import { useSidebar } from '@/components/layout/SidebarContext';
import {
  manageNavItems,
  type SidebarNavItem,
  workspaceNavItems,
} from '@/components/layout/sidebar-nav';
import {
  agentHasCatalogUpdate,
  sidebarInstallStats,
} from '@/components/layout/sidebar-stats';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { cn } from '@/lib/utils';
import { useI18n } from '@/components/shared/LanguageProvider';
import { useStoredIdOrder } from '@/components/shared/use-stored-id-order';
import { StorageKey } from '@/lib/ui-preferences';

const ICON_CLASS = 'h-4 w-4 shrink-0';

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
  const { to, navKey, icon: Icon } = item;
  const label = t(navKey);
  const tip = notice?.label;
  const a11yLabel = tip ? `${label} — ${tip}` : label;

  return (
    <NavLink
      to={to}
      end={to === '/'}
      aria-label={collapsed || tip ? a11yLabel : undefined}
      className="block rounded-btn focus:outline-none focus-visible:ring-1 focus-visible:ring-accent/30"
    >
      {({ isActive }) => {
        const node = (
          <span className={cn(itemClass(isActive), 'relative')}>
            <span className="relative shrink-0">
              <Icon className={ICON_CLASS} strokeWidth={1.8} />
              {/* Collapsed: corner pin on icon only (expanded uses trailing pin). */}
              {notice && collapsed && <StatusPin tone="warning" ring="panel" corner />}
            </span>
            {!collapsed && (
              <>
                <span className="truncate">{label}</span>
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

function agentDotLabel(
  meta: AgentMeta,
  status: AgentStatus | undefined,
  hasUpdate: boolean,
  upgradeable: string,
): string {
  const ver = status?.version ? ` v${status.version}` : '';
  const up = hasUpdate ? upgradeable : '';
  return `${meta.name}${ver}${up}`;
}

/** agent 在线状态迷你条：最底部 */
function SidebarAgentStrip({
  collapsed,
  agents,
  installedCount,
  visibleTotal,
  orderedInstalledMetas,
}: {
  collapsed: boolean;
  agents: readonly AgentStatus[];
  installedCount: number;
  visibleTotal: number;
  orderedInstalledMetas: readonly AgentMeta[];
}) {
  const { t } = useI18n();
  const fractionLabel = t('nav.agentsInstalled', {
    installed: installedCount,
    total: visibleTotal,
  });

  return (
    <div className={cn('shrink-0 border-t border-border', collapsed ? 'px-1.5 py-2.5' : 'px-3 py-2.5')}>
      {collapsed ? (
        <Hint label={fractionLabel} side="right">
          <div
            className="flex cursor-default flex-wrap items-center justify-center gap-1.5 rounded-btn py-0.5"
            aria-label={fractionLabel}
          >
            {orderedInstalledMetas.map((meta) => {
              const status = agents.find((row) => row.agentId === meta.id);
              const hasUpdate = agentHasCatalogUpdate(status);
              return (
                <AgentDot
                  key={meta.id}
                  agentId={meta.id}
                  color={meta.color}
                  title={null}
                  ring={!hasUpdate}
                  growOnHover
                  className={cn(hasUpdate && 'ring-2 ring-warning')}
                />
              );
            })}
          </div>
        </Hint>
      ) : (
        <div className="flex flex-wrap items-center gap-2">
          {orderedInstalledMetas.map((meta) => {
            const status = agents.find((row) => row.agentId === meta.id);
            const hasUpdate = agentHasCatalogUpdate(status);
            return (
              <AgentDot
                key={meta.id}
                agentId={meta.id}
                color={meta.color}
                title={agentDotLabel(
                  meta,
                  status,
                  hasUpdate,
                  t('nav.upgradeable', { version: status?.latestVersion ?? '' }),
                )}
                growOnHover
                className={cn(hasUpdate && 'ring-2 ring-warning')}
              />
            );
          })}
          {installedCount === 0 && (
            <span className="text-xs text-muted">{t('nav.noAgentInstalled')}</span>
          )}
          <span className="ml-auto shrink-0 text-xs text-muted">
            {installedCount}/{visibleTotal}
          </span>
        </div>
      )}
    </div>
  );
}

/** 侧边导航:可折叠;底部为 agent 在线状态迷你条 */
export function Sidebar() {
  const { collapsed, toggle, routesNavVisible, pluginsNavVisible } = useSidebar();
  const { t } = useI18n();
  const { statuses: agents } = useAgentStatusesOptional();
  const appUpdate = useAppUpdateAvailable();
  const settingsNotice = appUpdate
    ? { label: t('nav.updateAvailable', { version: appUpdate.version }) }
    : null;

  const itemClass = (isActive: boolean) =>
    cn(
      'group flex h-8 w-full items-center rounded-btn text-sm transition-colors duration-150',
      collapsed ? 'justify-center' : 'gap-2.5 px-2.5',
      // 与 ListRow / 预览 active 同源：中性 bg-active，非 accent 铺色
      isActive
        ? 'bg-active font-medium text-primary'
        : 'text-secondary hover:bg-hover/70 hover:text-primary',
    );

  const { stored: agentCatalogOrder } = useStoredIdOrder(StorageKey.agentsCatalogOrder);
  const stats = React.useMemo(
    () => sidebarInstallStats(AGENTS, agents, agentCatalogOrder),
    [agents, agentCatalogOrder],
  );
  const visibleWorkspaceNav = React.useMemo(
    () => workspaceNavItems(pluginsNavVisible),
    [pluginsNavVisible],
  );
  const visibleManageNav = React.useMemo(
    () => manageNavItems(routesNavVisible),
    [routesNavVisible],
  );

  return (
    <aside
      className={cn(
        pageRhythm.shellNav,
        collapsed ? 'w-14' : 'w-56',
        'transition-[width] duration-200 ease-in-out',
      )}
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
                <PanelLeftOpen className="h-4 w-4" strokeWidth={1.8} />
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
                <PanelLeftClose className="h-4 w-4" strokeWidth={1.8} />
              </button>
            </Hint>
          </>
        )}
      </div>

      {/* 工作区置顶；管理区 mt-auto 贴底（在 agent 状态条上方） */}
      <nav
        className={cn(
          'flex min-h-0 flex-1 flex-col gap-1 pt-1',
          collapsed ? 'px-1.5' : 'px-2',
        )}
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

      <SidebarAgentStrip
        collapsed={collapsed}
        agents={agents}
        installedCount={stats.installedCount}
        visibleTotal={stats.visibleTotal}
        orderedInstalledMetas={stats.orderedInstalledMetas}
      />
    </aside>
  );
}
