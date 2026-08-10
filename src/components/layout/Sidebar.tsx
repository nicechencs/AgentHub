import * as React from 'react';
import { NavLink } from 'react-router-dom';
import {
  Gauge,
  MessagesSquare,
  Bot,
  Key,
  Route,
  Blocks,
  Plug,
  FolderKanban,
  Settings2,
  PanelLeftClose,
  PanelLeftOpen,
  Hexagon,
} from 'lucide-react';
import { AgentDot } from '@/components/shared/AgentDot';
import { AGENTS } from '@/config/agents';
import { useAppUpdateAvailable } from '@/app/runtime';
import { listAgents } from '@/lib/api/agent';
import type { AgentStatus } from '@/lib/types';
import { Hint } from '@/components/ui/tooltip';
import { useSidebar } from '@/components/layout/SidebarContext';
import { cn } from '@/lib/utils';

/** 工作区 */
const NAV_WORKSPACE = [
  { to: '/chat', label: 'Chat', icon: MessagesSquare },
  { to: '/agents', label: 'Agents', icon: Bot },
  { to: '/skills', label: 'Skills', icon: Blocks },
  { to: '/mcp', label: 'MCP', icon: Plug },
  { to: '/projects', label: 'Projects', icon: FolderKanban },
] as const;

/** 管理 */
const NAV_MANAGE = [
  { to: '/', label: 'Dashboard', icon: Gauge },
  { to: '/connections', label: 'Connections', icon: Key },
  { to: '/router', label: 'Router', icon: Route },
  { to: '/settings', label: 'Settings', icon: Settings2 },
] as const;

type NavItem = (typeof NAV_WORKSPACE)[number] | (typeof NAV_MANAGE)[number];

const ICON_CLASS = 'h-4 w-4 shrink-0';

function SidebarNavLink({
  item,
  collapsed,
  itemClass,
  notice,
}: {
  item: NavItem;
  collapsed: boolean;
  itemClass: (isActive: boolean) => string;
  /** Optional silent tip (e.g. app update available on Settings). */
  notice?: { label: string } | null;
}) {
  const { to, label, icon: Icon } = item;
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
              {notice && collapsed && (
                <span
                  className="absolute -right-0.5 -top-0.5 h-1.5 w-1.5 rounded-full bg-warning ring-2 ring-panel"
                  aria-hidden
                />
              )}
            </span>
            {!collapsed && (
              <>
                <span className="truncate">{label}</span>
                {notice && (
                  <span
                    className="ml-auto h-1.5 w-1.5 shrink-0 rounded-full bg-warning"
                    aria-hidden
                    title={tip}
                  />
                )}
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
        <div className="px-2.5 pb-1 pt-2 text-2xs font-medium uppercase tracking-wide text-muted">
          {label}
        </div>
      )}
      {collapsed && <div className="h-2" aria-hidden />}
      {children}
    </div>
  );
}

function agentDotLabel(
  meta: (typeof AGENTS)[number],
  status: AgentStatus | undefined,
  hasUpdate: boolean,
): string {
  const ver = status?.version ? ` v${status.version}` : '';
  const up = hasUpdate ? ` (可升级 v${status?.latestVersion})` : '';
  return `${meta.name}${ver}${up}`;
}

/** 侧边导航:可折叠;底部为 agent 在线状态迷你条 */
export function Sidebar() {
  const { collapsed, toggle } = useSidebar();
  const [agents, setAgents] = React.useState<AgentStatus[]>([]);
  const appUpdate = useAppUpdateAvailable();
  const settingsNotice = appUpdate
    ? { label: `有可用更新 v${appUpdate.version}` }
    : null;

  React.useEffect(() => {
    listAgents().then(setAgents).catch(() => {});
  }, []);

  const installed = agents.filter((a) => a.installed).length;

  const itemClass = (isActive: boolean) =>
    cn(
      'group flex h-8 w-full items-center rounded-btn text-sm transition-colors duration-150',
      collapsed ? 'justify-center' : 'gap-2.5 px-2.5',
      // 与 ListRow / 预览 active 同源：中性 bg-active，非 accent 铺色
      isActive
        ? 'bg-active font-medium text-primary'
        : 'text-secondary hover:bg-hover/70 hover:text-primary',
    );

  const installedMetas = AGENTS.filter((meta) =>
    agents.some((a) => a.agentId === meta.id && a.installed),
  );

  return (
    <aside
      className={cn(
        'flex h-full shrink-0 flex-col border-r border-border bg-panel transition-[width] duration-200 ease-in-out',
        collapsed ? 'w-14' : 'w-56',
      )}
    >
      {/* 品牌 + 折叠按钮 */}
      <div
        className={cn(
          'flex h-10 shrink-0 items-center border-b border-border',
          collapsed ? 'justify-center' : 'justify-between px-3',
        )}
      >
        {collapsed ? (
          <Hint label="展开侧栏" side="right">
            <button
              type="button"
              onClick={toggle}
              className="group relative flex h-7 w-7 shrink-0 items-center justify-center rounded-btn focus:outline-none focus-visible:ring-1 focus-visible:ring-accent/30"
              aria-label="展开侧栏"
            >
              <span className="flex h-7 w-7 items-center justify-center rounded-btn bg-subtle text-secondary transition-opacity group-hover:opacity-0 group-focus-visible:opacity-0">
                <Hexagon className="h-4 w-4" strokeWidth={1.8} />
              </span>
              <span className="absolute inset-0 flex items-center justify-center rounded-btn text-muted opacity-0 transition-opacity group-hover:bg-hover group-hover:text-primary group-hover:opacity-100 group-focus-visible:bg-hover group-focus-visible:text-primary group-focus-visible:opacity-100">
                <PanelLeftOpen className="h-4 w-4" strokeWidth={1.8} />
              </span>
            </button>
          </Hint>
        ) : (
          <>
            <div className="flex min-w-0 items-center gap-2">
              <span className="flex h-6 w-6 shrink-0 items-center justify-center rounded-btn bg-subtle text-secondary">
                <Hexagon className="h-3.5 w-3.5" strokeWidth={1.8} />
              </span>
              <span className="truncate text-sm font-semibold tracking-tight">AgentHub</span>
            </div>
            <Hint label="收起侧栏" side="right">
              <button
                type="button"
                onClick={toggle}
                className="flex h-7 w-7 items-center justify-center rounded-btn text-muted transition-colors hover:bg-hover hover:text-primary"
                aria-label="收起侧栏"
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
        <NavGroup label="工作区" collapsed={collapsed}>
          {NAV_WORKSPACE.map((item) => (
            <SidebarNavLink key={item.to} item={item} collapsed={collapsed} itemClass={itemClass} />
          ))}
        </NavGroup>
        <NavGroup label="管理" collapsed={collapsed} className="mt-auto pb-2">
          {NAV_MANAGE.map((item) => (
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

      {/* agent 在线状态迷你条：最底部 */}
      <div className={cn('shrink-0 border-t border-border', collapsed ? 'px-1.5 py-2.5' : 'px-3 py-2.5')}>
        {collapsed ? (
          <Hint label={`${installed}/${AGENTS.length} agents 已安装`} side="right">
            <div
              className="flex cursor-default flex-wrap items-center justify-center gap-1.5 rounded-btn py-0.5"
              aria-label={`${installed}/${AGENTS.length} agents 已安装`}
            >
              {installedMetas.map((meta) => {
                const status = agents.find((a) => a.agentId === meta.id);
                const hasUpdate = Boolean(
                  status?.installed &&
                    status.latestVersion &&
                    status.version !== status.latestVersion,
                );
                return (
                  <AgentDot
                    key={meta.id}
                    agentId={meta.id}
                    color={meta.color}
                    title={null}
                    ring={!hasUpdate}
                    className={cn(hasUpdate && 'ring-2 ring-warning')}
                  />
                );
              })}
            </div>
          </Hint>
        ) : (
          <div className="flex flex-wrap items-center gap-2">
            {installedMetas.map((meta) => {
              const status = agents.find((a) => a.agentId === meta.id);
              const hasUpdate = Boolean(
                status?.installed &&
                  status.latestVersion &&
                  status.version !== status.latestVersion,
              );
              return (
                <AgentDot
                  key={meta.id}
                  agentId={meta.id}
                  color={meta.color}
                  title={agentDotLabel(meta, status, hasUpdate)}
                  className={cn(hasUpdate && 'ring-2 ring-warning')}
                />
              );
            })}
            {installed === 0 && (
              <span className="text-xs text-muted">未安装 Agent</span>
            )}
            <span className="ml-auto shrink-0 text-xs text-muted">
              {installed}/{AGENTS.length}
            </span>
          </div>
        )}
      </div>
    </aside>
  );
}
