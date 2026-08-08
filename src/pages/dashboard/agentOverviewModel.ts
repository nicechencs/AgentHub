import type { AgentMeta } from '@/config/agents';
import type { AgentStatus, AuthStatus } from '@/lib/types';

/** 内容与骨架屏共用：auto-fit 自适应，支持任意 agent 数量 */
export const AGENT_OVERVIEW_GRID =
  'grid gap-3 [grid-template-columns:repeat(auto-fit,minmax(190px,1fr))]';

/** 异常 = 未安装 / 环境未就绪 / 认证临期或失效 */
export function isAgentIssue(status: AgentStatus | undefined): boolean {
  if (!status || !status.installed) return true;
  if (status.envReady === false) return true;
  if (status.authStatus === 'expired' || status.authStatus === 'expiring') return true;
  return false;
}

export function summarizeAgentOverview(
  agentMetas: readonly AgentMeta[],
  agents: readonly AgentStatus[],
): { total: number; readyCount: number; issueCount: number; summaryText: string } {
  const issueCount = agentMetas.reduce((n, meta) => {
    const status = agents.find((a) => a.agentId === meta.id);
    return n + (isAgentIssue(status) ? 1 : 0);
  }, 0);
  const total = agentMetas.length;
  const readyCount = total - issueCount;
  const summaryText =
    issueCount > 0
      ? `${readyCount}/${total} 就绪 · ${issueCount} 项待处理`
      : `${readyCount}/${total} 就绪`;
  return { total, readyCount, issueCount, summaryText };
}

export function cardAuthStatus(
  status: AgentStatus | undefined,
  missing: boolean,
): AuthStatus {
  if (missing) return 'none';
  return status?.authStatus ?? 'none';
}

export interface AgentCardView {
  missing: boolean;
  envMissing: boolean;
  target: string;
  /** 已安装时展示在名称后的版本文案，如 `v2.1.218`；未安装为 null */
  versionText: string | null;
  metaText: string;
  metaClass: 'text-muted' | 'text-warning';
  titleFull: string;
  ariaLabel: string;
  statusDotTitle: string;
  authStatus: AuthStatus;
  /** 已装/未装两态均为 true：用于约束等高两行布局 */
  twoLineLayout: true;
}

/** 按 AGENTS 定义顺序生成卡片视图模型（不排序） */
export function buildAgentCardView(
  meta: AgentMeta,
  status: AgentStatus | undefined,
): AgentCardView {
  const missing = !status || !status.installed;
  const envMissing = missing && status?.envReady === false;

  // 当前生效：账号/密钥 → accounts；API 供应商 → providers；未配置默认进连接页
  const kind = status?.effectiveKind ?? 'none';
  const target = missing
    ? '/agents'
    : kind === 'account'
      ? `/connections?agent=${meta.id}`
      : kind === 'api'
        ? `/connections?mode=providers&agent=${meta.id}`
        : `/connections?agent=${meta.id}`;

  const effective =
    status?.effectiveLabel ?? status?.currentProvider ?? '未配置';
  const version = status?.version ?? '—';
  const versionText = missing ? null : `v${version}`;
  const authLabel = status?.authLabel || '—';

  let metaText: string;
  let metaClass: 'text-muted' | 'text-warning' = 'text-muted';
  if (missing) {
    if (envMissing) {
      metaText = '环境未就绪 · 点击修复';
      metaClass = 'text-warning';
    } else {
      metaText = '未安装 · 点击安装';
    }
  } else {
    metaText = effective;
  }

  const titleFull = missing ? metaText : `${effective} · ${authLabel}`;

  const connectionHint =
    kind === 'account' ? '当前账号/密钥' : kind === 'api' ? '当前 API 配置' : '当前连接';
  const ariaLabel = missing
    ? envMissing
      ? `${meta.name}，环境未就绪，点击修复`
      : `${meta.name}，未安装，点击安装`
    : `${meta.name}，${versionText}，${authLabel}，${connectionHint} ${effective}，点击管理连接`;

  const statusDotTitle = missing ? (envMissing ? '环境未就绪' : '未安装') : authLabel;

  return {
    missing,
    envMissing,
    target,
    versionText,
    metaText,
    metaClass,
    titleFull,
    ariaLabel,
    statusDotTitle,
    authStatus: cardAuthStatus(status, missing),
    twoLineLayout: true,
  };
}

/** 按 meta 顺序 merge 状态（位置稳定，不按异常重排） */
export function mergeAgentsInOrder(
  agentMetas: readonly AgentMeta[],
  agents: readonly AgentStatus[],
): Array<{ meta: AgentMeta; status: AgentStatus | undefined; view: AgentCardView }> {
  return agentMetas.map((meta) => {
    const status = agents.find((a) => a.agentId === meta.id);
    return { meta, status, view: buildAgentCardView(meta, status) };
  });
}
