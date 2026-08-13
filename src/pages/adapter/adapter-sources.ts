import { AGENT_MAP } from '@/config/agents';
import {
  AdapterCommandError,
  isAdapterErrorCodeRetryable,
  type AdapterBridgeRuntimeStatus,
  type AdapterProfileStatus,
} from '@/lib/backend/contracts/adapter';
import { isCapabilityUsable, type AgentCapabilities } from '@/lib/capability';
import type { AgentId } from '@/lib/types';
import type { ConnectionEntry } from '@/pages/connections/connection-model';
import {
  adapterBridgeStateLabel,
  adapterBridgeUpstreamLabel,
  errorMessage,
  sourceKindLabel,
  targetAgentName,
} from './adapter-model';

export type AdapterAgentStatusLike = {
  agentId: AgentId;
  installed?: boolean;
  capabilities?: AgentCapabilities;
};

export type AdapterAgentStatusState = 'idle' | 'loading' | 'ready' | 'error';

export type AdapterSourceGroup = {
  id: string;
  label: string;
  agentId: AgentId;
  source: ConnectionEntry['source'];
  entries: ConnectionEntry[];
};

const INCOMPLETE_OAUTH_HEALTH = new Set(['needs_login', 'missing']);
const INCOMPLETE_OAUTH_STATUS = new Set(['expired', 'none']);

/** Detect results are usable only after a completed snapshot exists. */
export function isAgentStatusUnavailable(
  state?: AdapterAgentStatusState | null,
  statuses?: ReadonlyArray<AdapterAgentStatusLike> | null,
): boolean {
  if (state === 'error' || state == null) return true;
  if (state === 'ready') return false;
  return !statuses || statuses.length === 0;
}

/** An adapter target is selectable when detect says installed, or config can clearly be written. */
export function isClearlyConfigurableAgent(status: AdapterAgentStatusLike): boolean {
  if (status.installed) return true;
  return isCapabilityUsable(status.capabilities?.configWrite)
    || isCapabilityUsable(status.capabilities?.accountSwitch);
}

/**
 * Target Agent options: installed / clearly configurable when detect is available.
 * Fall back to the catalog id list only when status cannot be used.
 */
export function selectableTargetAgentIds(input: {
  state?: AdapterAgentStatusState | null;
  statuses?: ReadonlyArray<AdapterAgentStatusLike> | null;
  fallbackIds: readonly AgentId[];
}): AgentId[] {
  const statuses = input.statuses ?? [];
  if (isAgentStatusUnavailable(input.state, statuses)) {
    return [...input.fallbackIds];
  }

  const byId = new Map(statuses.map((status) => [status.agentId, status]));
  const selected: AgentId[] = [];
  const seen = new Set<AgentId>();
  const consider = (agentId: AgentId) => {
    if (seen.has(agentId)) return;
    const status = byId.get(agentId);
    if (!status || !isClearlyConfigurableAgent(status)) return;
    selected.push(agentId);
    seen.add(agentId);
  };

  for (const agentId of input.fallbackIds) consider(agentId);
  for (const status of statuses) consider(status.agentId);
  return selected;
}

function agentSortIndex(agentId: AgentId, catalogOrder: readonly AgentId[]): number {
  const index = catalogOrder.indexOf(agentId);
  return index === -1 ? Number.MAX_SAFE_INTEGER : index;
}

function sourceKindSort(source: ConnectionEntry['source']): number {
  return source === 'account' ? 0 : 1;
}

/** Group Connections by Agent, then Account / Provider, preserving each group's relative order. */
export function groupAdapterSources(
  entries: readonly ConnectionEntry[],
  catalogOrder: readonly AgentId[] = [],
): AdapterSourceGroup[] {
  const groups = new Map<string, AdapterSourceGroup>();
  const order: string[] = [];

  for (const entry of entries) {
    const id = `${entry.agentId}:${entry.source}`;
    const existing = groups.get(id);
    if (existing) {
      existing.entries.push(entry);
      continue;
    }
    order.push(id);
    groups.set(id, {
      id,
      label: `${targetAgentName(entry.agentId)} · ${sourceKindLabel(entry.source)}`,
      agentId: entry.agentId,
      source: entry.source,
      entries: [entry],
    });
  }

  return order
    .map((id) => groups.get(id)!)
    .sort((left, right) => {
      const agentDelta = agentSortIndex(left.agentId, catalogOrder) - agentSortIndex(right.agentId, catalogOrder);
      if (agentDelta !== 0) return agentDelta;
      const nameDelta = (AGENT_MAP[left.agentId]?.name ?? left.agentId)
        .localeCompare(AGENT_MAP[right.agentId]?.name ?? right.agentId);
      if (nameDelta !== 0) return nameDelta;
      return sourceKindSort(left.source) - sourceKindSort(right.source);
    });
}

/** OAuth that still needs Connections-side login; Adapter must not fake an OAuth apply. */
export function isOAuthAuthIncomplete(
  entry: Pick<ConnectionEntry, 'kind' | 'authHealth' | 'authStatus'> | null | undefined,
): boolean {
  if (!entry || entry.kind !== 'oauth') return false;
  if (entry.authHealth && INCOMPLETE_OAUTH_HEALTH.has(entry.authHealth)) return true;
  return INCOMPLETE_OAUTH_STATUS.has(entry.authStatus);
}

export function oauthIncompleteAuthHint(): string {
  return '该官方登录尚未完成授权（需要重新登录、已过期或未登录）。请先到 Connections 完成授权；适配页不会代替发起 OAuth。';
}

export type AdapterApplyStage = 'idle' | 'applying' | 'active' | 'failed';

export function adapterApplyStage(input: {
  applying: boolean;
  successMessage?: string | null;
  error?: unknown;
  profileStatus?: AdapterProfileStatus | null;
}): AdapterApplyStage {
  if (input.applying) return 'applying';
  if (input.error) return 'failed';
  if (input.successMessage || input.profileStatus === 'active') return 'active';
  if (input.profileStatus === 'applying') return 'applying';
  return 'idle';
}

export function adapterApplyStageLabel(stage: AdapterApplyStage): string | null {
  if (stage === 'applying') return '应用中';
  if (stage === 'active') return '已生效';
  if (stage === 'failed') return '应用失败';
  return null;
}

export function isAdapterFailureRetryable(error: unknown): boolean {
  if (error instanceof AdapterCommandError) return error.retryable;
  if (error && typeof error === 'object' && 'retryable' in error) {
    return (error as { retryable?: unknown }).retryable === true;
  }
  if (error && typeof error === 'object' && 'code' in error && typeof (error as { code?: unknown }).code === 'string') {
    return isAdapterErrorCodeRetryable((error as { code: string }).code);
  }
  return false;
}

export function adapterFailurePresentation(error: unknown, fallback: string): {
  message: string;
  retryable: boolean;
  hint: string;
} {
  const retryable = isAdapterFailureRetryable(error);
  return {
    message: errorMessage(error, fallback),
    retryable,
    hint: retryable
      ? '可重试此次操作；不会自动反复重试写配置。'
      : '此失败不可重试写入。请检查来源连接或删除后重新创建；不会自动重试。',
  };
}

export function adapterBridgeProbeSummary(status?: AdapterBridgeRuntimeStatus | null): string | null {
  if (!status) return null;
  const upstream = status.upstreamStatus
    ? adapterBridgeUpstreamLabel(status.upstreamStatus)
    : '未返回';
  return `本地桥接检查：${adapterBridgeStateLabel(status.state)} · 上游 ${upstream}`;
}

export function adapterProfilePortLabel(
  profile: { localPort?: number | null },
  status?: AdapterBridgeRuntimeStatus | null,
): string {
  const port = status?.port ?? profile.localPort;
  return port ? `127.0.0.1:${port}` : '待分配端口';
}

export function adapterProfileLastErrorCode(profile: { lastErrorCode?: string | null }): string | null {
  const code = profile.lastErrorCode?.trim();
  return code ? code : null;
}

export function adapterNeedsAttentionRecovery(
  profile: { status: AdapterProfileStatus; route: string; lastErrorCode?: string | null },
  bridgeState?: AdapterBridgeRuntimeStatus['state'],
): { hint: string; startLabel: string | null; canStart: boolean; canDelete: true } {
  const lastError = adapterProfileLastErrorCode(profile);
  const retryStart = profile.route === 'local_bridge' && (bridgeState === 'error' || lastError != null);
  return {
    hint: lastError
      ? `需要处理（${lastError}）：可手动启动、重试启动或删除后重新创建；不会自动反复重试写配置。`
      : '需要处理：上次操作可能部分完成。请手动启动 / 重试启动，或删除后重新创建；不会自动反复重试。',
    startLabel: profile.route === 'local_bridge'
      ? (retryStart ? '重试启动' : '启动')
      : null,
    canStart: profile.route === 'local_bridge',
    canDelete: true,
  };
}

