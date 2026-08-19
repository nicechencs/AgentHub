import type { AgentMeta } from '@/config/agents';
import { authDisplayForAgentStatus } from '@/lib/backend/contracts/auth-state';
import type { TranslateFn } from '@/lib/i18n';
import type { AgentId, AgentStatus, AuthStatus } from '@/lib/types';

/** 内容与骨架屏共用：auto-fit 自适应，支持任意 agent 数量 */
export const AGENT_OVERVIEW_GRID =
  'grid gap-3 [grid-template-columns:repeat(auto-fit,minmax(190px,1fr))]';

/** 已安装 → 打开连接流程；未安装 / 环境未就绪 → 跳转 Agents 页 */
export type AgentCardAction =
  | { kind: 'connect' }
  | { kind: 'navigate'; to: string };

/** 卡片桥徽标态（调用方已把查询失败收敛为 unavailable） */
export type AgentCardBridgeState = 'running' | 'stopped' | 'degraded' | 'unavailable';

/**
 * 徽标输入：由调用方完成 profile 联结与桥状态查询后传入。
 * 模型层只做映射，不读 provider.meta、不发请求。
 */
export interface AgentCardBadgeInput {
  /** 当前生效 provider.id 命中 AdapterProfile.generatedProviderId 时传入 */
  viaAdapter?: { sourceLabel?: string } | null;
  /** 命中的 profile 为 bridge 型时传入；查询失败传 unavailable，不得省略 */
  bridge?: { state: AgentCardBridgeState; profileId?: string | null } | null;
  /**
   * 当前绑定的票（钱包读模型）。有值时卡片主文案优先展示
   * 「票 label · 直连/改配置/本机路由」，不再只显示 Provider 行名。
   */
  binding?: { ticketLabel: string; routeLabel: string } | null;
}

export const AGENT_CARD_BRIDGE_LABEL: Record<AgentCardBridgeState, string> = {
  running: '运行中',
  stopped: '已停止',
  degraded: '已降级',
  unavailable: '状态不可用',
};

export function agentCardBridgeLabel(state: AgentCardBridgeState, t?: TranslateFn): string {
  if (t) {
    if (state === 'running') return t('routes.runtime.running');
    if (state === 'stopped') return t('routes.runtime.stopped');
    if (state === 'degraded') return t('routes.runtime.degraded');
    return t('routes.runtime.unavailable');
  }
  return AGENT_CARD_BRIDGE_LABEL[state];
}

/** 异常 = 未安装 / 环境未就绪 / 认证临期或失效 */
export function isAgentIssue(status: AgentStatus | undefined): boolean {
  if (!status || !status.installed) return true;
  if (status.envReady === false) return true;
  const display = authDisplayForAgentStatus(status);
  if (display.health === 'needs_login') return true;
  // Legacy doctor rows may still report expiring; explicit renewable health
  // must take precedence so Dashboard does not call it an outage.
  if (status.authHealth) return false;
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

export function cardAuthStatus(
  status: AgentStatus | undefined,
  missing: boolean,
): AuthStatus {
  if (missing) return 'none';
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
  /** 当前生效连接经 Adapter 投影时非空 */
  viaAdapter?: { sourceLabel?: string };
  /** 命中 bridge 型 profile 时非空；查询失败为 unavailable，不得省略 */
  bridge?: { state: AgentCardBridgeState; label: string; profileId: string | null };
  /** 钱包读模型：当前绑定的票 */
  binding?: { ticketLabel: string; routeLabel: string };
}

function mapViaAdapter(
  input?: { sourceLabel?: string } | null,
): { sourceLabel?: string } | undefined {
  if (!input) return undefined;
  const sourceLabel = input.sourceLabel?.trim();
  return sourceLabel ? { sourceLabel } : {};
}

function mapBridgeBadge(
  input?: { state: AgentCardBridgeState; profileId?: string | null } | null,
  t?: TranslateFn,
): { state: AgentCardBridgeState; label: string; profileId: string | null } | undefined {
  if (!input) return undefined;
  return {
    state: input.state,
    label: agentCardBridgeLabel(input.state, t),
    profileId: input.profileId ?? null,
  };
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

  const kind = status?.effectiveKind ?? 'none';
  const unconfigured = t ? t('dashboard.overview.unconfigured') : '未配置';
  const effective =
    status?.effectiveLabel ?? status?.currentProvider ?? unconfigured;
  const version = status?.version ?? '—';
  const versionText = missing ? null : `v${version}`;
  const authDisplay = authDisplayForAgentStatus(status);
  const authLabel = (status?.authHealth ? authDisplay.label : status?.authLabel) || '—';

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
    metaText = `${badges.binding.ticketLabel} · ${badges.binding.routeLabel}`;
  } else {
    metaText = effective;
  }

  const titleFull = missing
    ? metaText
    : badges?.binding?.ticketLabel
      ? `${badges.binding.ticketLabel} · ${badges.binding.routeLabel} · ${authLabel}`
      : `${effective} · ${authLabel}`;

  const connectionHint =
    kind === 'account'
      ? t
        ? t('dashboard.overview.hintAccount')
        : '当前账号/密钥'
      : kind === 'api'
        ? t
          ? t('dashboard.overview.hintApi')
          : '当前 API 配置'
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

  const viaAdapter = mapViaAdapter(badges?.viaAdapter);
  const bridge = mapBridgeBadge(badges?.bridge, t);
  let authStatus = cardAuthStatus(status, missing);
  if (!missing && bridge?.state === 'stopped' && authStatus === 'valid') {
    authStatus = 'none';
  }
  const binding = badges?.binding?.ticketLabel
    ? {
        ticketLabel: badges.binding.ticketLabel,
        routeLabel: badges.binding.routeLabel,
      }
    : undefined;
  if (viaAdapter) {
    ariaLabel += viaAdapter.sourceLabel
      ? t
        ? t('dashboard.overview.ariaViaSource', { source: viaAdapter.sourceLabel })
        : `，本机路由 · ${viaAdapter.sourceLabel}`
      : t
        ? t('dashboard.overview.ariaVia')
        : '，本机路由';
  }
  if (binding) {
    ariaLabel += t
      ? t('dashboard.overview.ariaBinding', { label: binding.ticketLabel, route: binding.routeLabel })
      : `，当前绑定 ${binding.ticketLabel}（${binding.routeLabel}）`;
  }
  if (bridge) {
    ariaLabel += t ? t('dashboard.overview.ariaBridge', { label: bridge.label }) : `，${bridge.label}`;
  }

  let statusDotTitle = missing
    ? envMissing
      ? t
        ? t('dashboard.overview.envNotReadyShort')
        : '环境未就绪'
      : t
        ? t('dashboard.overview.notInstalledShort')
        : '未安装'
    : authLabel;
  if (!missing && bridge?.state === 'stopped') {
    statusDotTitle = agentCardBridgeLabel('stopped', t);
  }

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
    authHealth: authDisplay.health,
    twoLineLayout: true,
    ...(viaAdapter ? { viaAdapter } : {}),
    ...(binding ? { binding } : {}),
    ...(bridge ? { bridge } : {}),
  };
}

/** Dashboard index 已接线 onConnectRequest；未传回调时 connect 退化为 Connections 页 */
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
