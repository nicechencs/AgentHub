import { resolveAgentMeta, type AgentMeta } from '@/config/agents';
import { sliceAgentStatus } from '@/lib/backend/contracts/agent-status-view';
import {
  authDisplayForAgentStatus,
  authHealthLabel,
} from '@/lib/backend/contracts/auth-state';
import type { BindingRoute } from '@/lib/backend/contracts/ticket';
import type { TranslateFn } from '@/lib/i18n';
import { localizeStoredUiCopy } from '@/lib/i18n/stored-copy';
import {
  formatRouteEndpointHttpUrl,
  routeEndpointPathForBinding,
} from '@/lib/route-endpoints';
import { connectionStateRouteLabel } from '@/lib/ticket-wallet-labels';
import type { AgentId, AgentStatus, AuthStatus } from '@/lib/types';

/** Backend/store rows keep Chinese literals. Remap at display time when `t` is set. */
export function localizeStoredDashboardCopy(raw: string, t?: TranslateFn): string {
  return localizeStoredUiCopy(raw, t);
}

/** 内容与骨架屏共用：auto-fit 自适应，支持任意 agent 数量 */
export const AGENT_OVERVIEW_GRID =
  'grid gap-3 [grid-template-columns:repeat(auto-fit,minmax(190px,1fr))]';

/** 已安装 → 打开连接流程；未安装 / 环境未就绪 → 跳转 Agents 页 */
export type AgentCardAction =
  | { kind: 'connect' }
  | { kind: 'navigate'; to: string };

/**
 * 当前正在用的授权。模型层只做映射，不读 provider.meta、不发请求。
 */
export interface AgentCardBadgeInput {
  /**
   * 当前绑定的登录。有值时卡片主文案优先展示
   * 「登录」或「登录 · 改配置」或「登录 · 127.0.0.1:端口/路径」。
   */
  binding?: { ticketLabel: string; routeLabel: string } | null;
}

const LOCAL_ROUTE_MARKERS = ['本机路由', 'Local route'] as const;

/** 总览只展示授权本身；落盘里的「本机路由 · …」前缀去掉。 */
export function dashboardConnectionLabel(raw: string, t?: TranslateFn): string {
  const localized = localizeStoredDashboardCopy(raw, t);
  const markers = new Set<string>(LOCAL_ROUTE_MARKERS);
  if (t) markers.add(t('kind.route.localRoute'));
  for (const marker of markers) {
    if (localized === marker) {
      return t ? t('dashboard.overview.hintConnection') : '当前连接';
    }
    const prefix = `${marker} · `;
    if (localized.startsWith(prefix)) {
      return localized.slice(prefix.length).trim()
        || (t ? t('dashboard.overview.hintConnection') : '当前连接');
    }
  }
  return localized;
}

/** 走路由时用 127.0.0.1 地址，不用「本机路由」。 */
export function dashboardBindingMeta(
  input: {
    ticketLabel: string;
    route: BindingRoute;
    agentId: AgentId;
    port?: number | null;
  },
  t?: TranslateFn,
): { ticketLabel: string; routeLabel: string } {
  const ticketLabel = dashboardConnectionLabel(input.ticketLabel, t);
  if (input.route === 'bridge') {
    return {
      ticketLabel,
      routeLabel: formatRouteEndpointHttpUrl({
        path: routeEndpointPathForBinding({ agentId: input.agentId }),
        port: input.port ?? null,
      }),
    };
  }
  return {
    ticketLabel,
    routeLabel: connectionStateRouteLabel(input.route, t),
  };
}

/** 异常 = 未安装 / 环境未就绪 / 认证临期或失效 */
export function isAgentIssue(status: AgentStatus | undefined): boolean {
  if (!status || !status.installed) return true;
  const view = sliceAgentStatus(status);
  if (view.env.ready === false) return true;
  if (view.liveAuth.health === 'needs_login') return true;
  if (view.liveAuth.health !== 'unset') return false;
  if (status.authStatus === 'expired' || status.authStatus === 'expiring') return true;
  return false;
}

export function summarizeAgentOverview(
  agentMetas: readonly AgentMeta[],
  agents: readonly AgentStatus[],
  t?: TranslateFn,
): { total: number; readyCount: number; issueCount: number; summaryText: string } {
  const issueCount = agentMetas.reduce((n, meta) => {
    const status = agents.find((a) => a.agentId === meta.id);
    return n + (isAgentIssue(status) ? 1 : 0);
  }, 0);
  const total = agentMetas.length;
  const readyCount = total - issueCount;
  const summaryText = t
    ? issueCount > 0
      ? t('dashboard.overview.summaryIssues', { ready: readyCount, total, issues: issueCount })
      : t('dashboard.overview.summaryReady', { ready: readyCount, total })
    : issueCount > 0
      ? `${readyCount}/${total} 就绪 · ${issueCount} 项待处理`
      : `${readyCount}/${total} 就绪`;
  return { total, readyCount, issueCount, summaryText };
}

/** Dashboard 卡片只含已安装且未隐藏的 Agent；未安装的去 Agents 页。 */
export function installedOverviewScope(
  agentMetas: readonly AgentMeta[],
  agents: readonly AgentStatus[],
): { metas: AgentMeta[]; statuses: AgentStatus[] } {
  const statuses = agents.filter(
    (a) => a.installed && sliceAgentStatus(a).hidden !== 'hidden',
  );
  if (agentMetas.length === 0) {
    // Catalog not hydrated / failed: still show doctor-detected rows.
    return {
      metas: statuses.map((row) => resolveAgentMeta(row.agentId)),
      statuses,
    };
  }
  const ids = new Set(statuses.map((a) => a.agentId));
  return {
    metas: agentMetas.filter((m) => ids.has(m.id)),
    statuses,
  };
}

/**
 * 页头副标题：把就绪计数并进「状态与用量」。
 * 加载中、失败、尚未安装时不加括号，避免 0/0。
 */
export function dashboardPageDescription(
  summary: { total: number; summaryText: string } | null,
  t?: TranslateFn,
): string {
  if (!summary || summary.total === 0) {
    return t ? t('dashboard.page.description') : '状态与用量';
  }
  return t
    ? t('dashboard.page.descriptionWithSummary', { summary: summary.summaryText })
    : `状态与用量（${summary.summaryText}）`;
}

export function cardAuthStatus(
  status: AgentStatus | undefined,
  missing: boolean,
): AuthStatus {
  if (missing || !status) return 'none';
  const health = sliceAgentStatus(status).liveAuth.health;
  if (health !== 'unset') {
    return authDisplayForAgentStatus({ ...status, authHealth: health }).legacyStatus;
  }
  return authDisplayForAgentStatus(status).legacyStatus;
}

export interface AgentCardView {
  missing: boolean;
  envMissing: boolean;
  action: AgentCardAction;
  /** 已安装时展示在名称后的版本文案，如 `v2.1.218`；未安装为 null */
  versionText: string | null;
  metaText: string;
  metaClass: 'text-muted' | 'text-warning';
  titleFull: string;
  ariaLabel: string;
  statusDotTitle: string;
  authStatus: AuthStatus;
  authHealth: ReturnType<typeof authDisplayForAgentStatus>['health'];
  /** 已装/未装两态均为 true：用于约束等高两行布局 */
  twoLineLayout: true;
  /** 当前正在用的授权 */
  binding?: { ticketLabel: string; routeLabel: string };
}

/** 按 AGENTS 定义顺序生成卡片视图模型（不排序） */
export function buildAgentCardView(
  meta: AgentMeta,
  status: AgentStatus | undefined,
  badges?: AgentCardBadgeInput | null,
  t?: TranslateFn,
): AgentCardView {
  const missing = !status || !status.installed;
  const envMissing = missing && status?.envReady === false;

  // 已安装：打开连接流程；未安装 / 环境未就绪：保留跳转 /agents
  const action: AgentCardAction = missing
    ? { kind: 'navigate', to: '/agents' }
    : { kind: 'connect' };

  const view = sliceAgentStatus(status ?? {});
  const kind = view.effectiveConnection.kind === 'unset' ? 'none' : view.effectiveConnection.kind;
  const unconfigured = t ? t('dashboard.overview.unconfigured') : '未配置';
  const rawEffective =
    view.effectiveConnection.label !== 'unset'
      ? view.effectiveConnection.label
      : unconfigured;
  const effective = dashboardConnectionLabel(rawEffective, t);
  const version = status?.version ?? '—';
  const versionText = missing ? null : `v${version}`;
  const rawAuthLabel =
    view.liveAuth.health === 'unset'
      ? (status?.authLabel || '—')
      : authHealthLabel(view.liveAuth.health, t);
  const authLabel = localizeStoredDashboardCopy(rawAuthLabel, t);

  let metaText: string;
  let metaClass: 'text-muted' | 'text-warning' = 'text-muted';
  if (missing) {
    if (envMissing) {
      metaText = t ? t('dashboard.overview.envNotReady') : '环境未就绪 · 点击修复';
      metaClass = 'text-warning';
    } else {
      metaText = t ? t('dashboard.overview.notInstalled') : '未安装 · 点击安装';
    }
  } else if (badges?.binding?.ticketLabel) {
    metaText = badges.binding.routeLabel
      ? `${badges.binding.ticketLabel} · ${badges.binding.routeLabel}`
      : badges.binding.ticketLabel;
  } else {
    metaText = effective;
  }

  const titleFull = missing
    ? metaText
    : badges?.binding?.ticketLabel
      ? badges.binding.routeLabel
        ? `${badges.binding.ticketLabel} · ${badges.binding.routeLabel} · ${authLabel}`
        : `${badges.binding.ticketLabel} · ${authLabel}`
      : `${effective} · ${authLabel}`;

  const connectionHint =
    kind === 'account'
      ? t
        ? t('dashboard.overview.hintAccount')
        : 'Official login'
      : kind === 'api'
        ? t
          ? t('dashboard.overview.hintApi')
          : 'API Key'
        : t
          ? t('dashboard.overview.hintConnection')
          : '当前连接';
  let ariaLabel = missing
    ? envMissing
      ? t
        ? t('dashboard.overview.ariaMissingEnv', { name: meta.name })
        : `${meta.name}，环境未就绪，点击修复`
      : t
        ? t('dashboard.overview.ariaMissing', { name: meta.name })
        : `${meta.name}，未安装，点击安装`
    : t
      ? t('dashboard.overview.ariaInstalled', {
          name: meta.name,
          version: versionText ?? `v${version}`,
          auth: authLabel,
          hint: connectionHint,
          effective,
        })
      : `${meta.name}，${versionText}，${authLabel}，${connectionHint} ${effective}，点击管理连接`;

  const authStatus = cardAuthStatus(status, missing);
  const binding = badges?.binding?.ticketLabel
    ? {
        ticketLabel: dashboardConnectionLabel(badges.binding.ticketLabel, t),
        routeLabel: dashboardConnectionLabel(badges.binding.routeLabel, t),
      }
    : undefined;
  if (binding) {
    ariaLabel += t
      ? t('dashboard.overview.ariaBinding', {
          label: binding.ticketLabel,
          route: binding.routeLabel || binding.ticketLabel,
        })
      : binding.routeLabel
        ? `，当前使用 ${binding.ticketLabel}（${binding.routeLabel}）`
        : `，当前使用 ${binding.ticketLabel}`;
  }

  const statusDotTitle = missing
    ? envMissing
      ? t
        ? t('dashboard.overview.envNotReadyShort')
        : '环境未就绪'
      : t
        ? t('dashboard.overview.notInstalledShort')
        : '未安装'
    : authLabel;

  return {
    missing,
    envMissing,
    action,
    versionText,
    metaText,
    metaClass,
    titleFull,
    ariaLabel,
    statusDotTitle,
    authStatus,
    authHealth: view.liveAuth.health === 'unset' ? 'unknown' : view.liveAuth.health,
    twoLineLayout: true,
    ...(binding ? { binding } : {}),
  };
}

/** 未传 onConnectRequest 时 connect 退化为 Connections 页 */
export function agentCardConnectFallback(agentId: AgentId): string {
  return `/connections?agent=${agentId}`;
}

/**
 * 卡片交互决议：navigate 原样跳转；connect 有回调则发请求，否则退化为 Connections。
 */
export function resolveAgentCardInteraction(
  action: AgentCardAction,
  agentId: AgentId,
  onConnectRequest?: (agentId: AgentId) => void,
): { type: 'connect'; agentId: AgentId } | { type: 'navigate'; to: string } {
  if (action.kind === 'navigate') {
    return { type: 'navigate', to: action.to };
  }
  if (onConnectRequest) {
    return { type: 'connect', agentId };
  }
  return { type: 'navigate', to: agentCardConnectFallback(agentId) };
}

/** Loading 骨架卡数 = 已装且未隐藏的 Agent，不用目录全量。 */
export function dashboardOverviewSkeletonCount(
  agents: AgentStatus[] | null,
  fallbackInstalledCount: number,
): number {
  if (agents) {
    return agents.filter((a) => a.installed && !a.hidden).length;
  }
  return Math.max(0, fallbackInstalledCount);
}

/** 按 meta 顺序 merge 状态（位置稳定，不按异常重排） */
export function mergeAgentsInOrder(
  agentMetas: readonly AgentMeta[],
  agents: readonly AgentStatus[],
  badgeInputs?: Readonly<Partial<Record<AgentId, AgentCardBadgeInput>>> | null,
  t?: TranslateFn,
): Array<{ meta: AgentMeta; status: AgentStatus | undefined; view: AgentCardView }> {
  return agentMetas.map((meta) => {
    const status = agents.find((a) => a.agentId === meta.id);
    return { meta, status, view: buildAgentCardView(meta, status, badgeInputs?.[meta.id], t) };
  });
}
