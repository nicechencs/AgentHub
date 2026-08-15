/**
 * Pure view-model helpers for the redesigned Adapter page:
 * target fan-out cards, two-layer profile status, source resolution,
 * and the route pipeline. No IO, no React.
 */
import { agentDisplayName } from '@/config/agents';
import type {
  AdapterBridgeRuntimeState,
  AdapterProfile,
  AdapterProfileStatus,
  AdapterRoute,
  AdapterRouteAnalysis,
  AdapterSourceKind,
} from '@/lib/backend/contracts/adapter';
import type { AgentId } from '@/lib/types';
import type { ConnectionEntry } from '@/pages/connections/connection-model';

// ---------- Onboarding ----------

/**
 * Static quick-start examples for the "nothing selected yet" pane.
 * Product guidance only — the capability matrix in core stays the sole
 * authority; every real conclusion still comes from `analyze`.
 */
export const ADAPTER_SUPPORTED_PATH_EXAMPLES: ReadonlyArray<{
  source: string;
  targetAgentId: AgentId;
  badge: { label: string; variant: 'success' | 'warning' };
}> = [
  { source: 'Kimi Code 会员', targetAgentId: 'claude', badge: { label: '直连', variant: 'success' } },
  { source: 'Kimi Code 会员', targetAgentId: 'codex', badge: { label: '桥接 · 实验', variant: 'warning' } },
];

// ---------- Target fan-out ----------

/** One target card. `unconfigurable` means detect says not installed / not writable. */
export type AdapterTargetAnalysisState =
  | { kind: 'unconfigurable' }
  | { kind: 'loading' }
  | { kind: 'ready'; analysis: AdapterRouteAnalysis }
  | { kind: 'error'; error: unknown };

/** Session cache key for one analyzed source × target pair. */
export function adapterTargetCacheKey(input: {
  sourceKind: AdapterSourceKind;
  sourceId: string;
  targetAgentId: AgentId;
}): string {
  return `${input.sourceKind}:${input.sourceId}:${input.targetAgentId}`;
}

/**
 * Route conclusion badge for one target card. This mirrors `analyze` only —
 * it never implies write access; Apply is still gated by `plan.canApply`.
 */
export function adapterTargetBadge(
  analysis: Pick<AdapterRouteAnalysis, 'route' | 'support'>,
): { label: string; variant: 'success' | 'warning' | 'info' | 'default' } {
  if (analysis.route === 'native_endpoint') return { label: '直连', variant: 'success' };
  if (analysis.route === 'local_bridge') {
    return analysis.support === 'experimental'
      ? { label: '桥接 · 实验', variant: 'warning' }
      : { label: '本地桥接', variant: 'warning' };
  }
  if (analysis.route === 'config_sync') return { label: '配置同步', variant: 'info' };
  return { label: '暂不支持', variant: 'default' };
}

// ---------- Two-layer profile status ----------

export type AdapterStatusTone = 'success' | 'warning' | 'danger' | 'info' | 'muted';

/** `pulse` marks transient states (applying/starting/stopping) for a breathing dot. */
export type AdapterStatusView = { label: string; tone: AdapterStatusTone; pulse?: boolean };

/** Durable configuration lifecycle. Never mixed with bridge runtime. */
export function adapterConfigStatusView(status: AdapterProfileStatus): AdapterStatusView {
  if (status === 'active') return { label: '配置已生效', tone: 'success' };
  if (status === 'applying') return { label: '应用中', tone: 'info', pulse: true };
  return { label: '需要处理', tone: 'warning' };
}

/**
 * Single-layer local-bridge runtime status. A failed status read is
 * "状态不可用" (the listener may still be fine) and must not be presented as
 * a start failure — even when last-known `bridgeState` is still `running`.
 */
export function bridgeRuntimeStatusView(input: {
  route: AdapterProfile['route'];
  bridgeState?: AdapterBridgeRuntimeState;
  statusUnavailable?: boolean;
}): AdapterStatusView | null {
  if (input.route !== 'local_bridge') return null;
  if (input.statusUnavailable) return { label: '状态不可用', tone: 'muted' };
  const state = input.bridgeState;
  if (state === 'running') return { label: '运行中', tone: 'success' };
  if (state === 'starting') return { label: '启动中', tone: 'info', pulse: true };
  if (state === 'stopping') return { label: '停止中', tone: 'info', pulse: true };
  if (state === 'degraded') return { label: '已降级', tone: 'warning' };
  if (state === 'error') return { label: '启动失败', tone: 'danger' };
  return { label: '已停止', tone: 'muted' };
}

/** @deprecated Prefer {@link bridgeRuntimeStatusView}; same single-layer labels. */
export function adapterServiceStatusView(input: {
  route: AdapterProfile['route'];
  bridgeState?: AdapterBridgeRuntimeState;
  statusUnavailable?: boolean;
}): AdapterStatusView | null {
  return bridgeRuntimeStatusView(input);
}

export function adapterStatusDotClass(tone: AdapterStatusTone): string {
  if (tone === 'success') return 'bg-success';
  if (tone === 'warning') return 'bg-warning';
  if (tone === 'danger') return 'bg-danger';
  if (tone === 'info') return 'bg-info';
  return 'bg-muted';
}

export function adapterStatusTextClass(tone: AdapterStatusTone): string {
  if (tone === 'success') return 'text-success';
  if (tone === 'warning') return 'text-warning';
  if (tone === 'danger') return 'text-danger';
  if (tone === 'info') return 'text-info';
  return 'text-muted';
}

// ---------- Profile source resolution ----------

export type AdapterProfileSourceView = {
  title: string;
  agentId: AgentId | null;
  /** True when the source connection no longer exists in the pool. */
  missing: boolean;
};

/**
 * Resolve the human-readable source connection of a saved profile.
 * Match by (sourceKind, sourceId): account and provider row ids may collide.
 */
export function resolveAdapterProfileSource(
  profile: Pick<AdapterProfile, 'sourceKind' | 'sourceId' | 'name'>,
  entries: readonly Pick<ConnectionEntry, 'source' | 'id' | 'title' | 'agentId'>[],
): AdapterProfileSourceView {
  const match = entries.find(
    (entry) => entry.source === profile.sourceKind && entry.id === profile.sourceId,
  );
  if (match) return { title: match.title, agentId: match.agentId, missing: false };
  return { title: profile.name, agentId: null, missing: true };
}

export type BridgePartition<T extends AdapterProfile = AdapterProfile> = {
  /** Source still exists, or last-known wallet `binding.profileId` hits. */
  bound: T[];
  /** Remaining `local_bridge` rows with a non-empty sourceId. */
  orphan: T[];
};

function isLocalBridgeRuntime(
  profile: Pick<AdapterProfile, 'route' | 'sourceId'>,
): boolean {
  return profile.route === 'local_bridge' && Boolean(profile.sourceId.trim());
}

function sourceStillPresent(
  profile: Pick<AdapterProfile, 'sourceKind' | 'sourceId'>,
  entries: readonly Pick<ConnectionEntry, 'source' | 'id'>[],
): boolean {
  return entries.some(
    (entry) => entry.source === profile.sourceKind && entry.id === profile.sourceId,
  );
}

/** Bound `local_bridge` only (source present or last-known binding hit). */
export function isBoundLocalBridgeRuntime(
  profile: Pick<AdapterProfile, 'id' | 'route' | 'sourceKind' | 'sourceId'>,
  input: {
    entries: readonly Pick<ConnectionEntry, 'source' | 'id'>[];
    bindingProfileIds?: ReadonlySet<string>;
  },
): boolean {
  if (!isLocalBridgeRuntime(profile)) return false;
  if (sourceStillPresent(profile, input.entries)) return true;
  return input.bindingProfileIds?.has(profile.id) === true;
}

/** All `local_bridge` runtimes: bound vs orphan. Dirty rows with empty sourceId are dropped. */
export function partitionLocalBridgeRuntimes<T extends AdapterProfile>(
  profiles: readonly T[],
  input: {
    entries: readonly Pick<ConnectionEntry, 'source' | 'id'>[];
    bindingProfileIds?: ReadonlySet<string>;
  },
): BridgePartition<T> {
  const bound: T[] = [];
  const orphan: T[] = [];
  for (const profile of profiles) {
    if (!isLocalBridgeRuntime(profile)) continue;
    if (sourceStillPresent(profile, input.entries) || input.bindingProfileIds?.has(profile.id)) {
      bound.push(profile);
    } else {
      orphan.push(profile);
    }
  }
  return { bound, orphan };
}

/** @deprecated Prefer {@link partitionLocalBridgeRuntimes}; this is `.bound` only. */
export function filterBoundLocalBridgeRuntimes<T extends AdapterProfile>(
  profiles: readonly T[],
  input: {
    entries: readonly Pick<ConnectionEntry, 'source' | 'id'>[];
    bindingProfileIds?: ReadonlySet<string>;
  },
): T[] {
  return partitionLocalBridgeRuntimes(profiles, input).bound;
}

export type BridgesWalletView = {
  /** At least one wallet fetch finished (success, or failure with a last/empty snapshot). */
  settled: boolean;
  /** Last successful `route=bridge` count; a later failure must not write 0. */
  lastWalletBridgeCount: number;
};

export type BridgesPageViewState =
  | 'loading'
  | 'list_error'
  | 'list'
  | 'wallet_without_runtime'
  | 'healthy_empty';

export function bridgesPageViewState(input: {
  profileState: 'loading' | 'ready' | 'error';
  bound: readonly unknown[];
  orphan: readonly unknown[];
  wallet: BridgesWalletView;
}): BridgesPageViewState {
  if (input.profileState === 'loading' || !input.wallet.settled) return 'loading';
  if (input.profileState === 'error') return 'list_error';
  if (input.bound.length + input.orphan.length > 0) return 'list';
  if (input.wallet.lastWalletBridgeCount > 0) return 'wallet_without_runtime';
  return 'healthy_empty';
}

// ---------- Route pipeline ----------

export type AdapterPipelineNode = {
  kind: 'source' | 'bridge' | 'target';
  title: string;
  subtitle: string;
  agentId?: AgentId;
};

export type AdapterPipelineModel = {
  nodes: AdapterPipelineNode[];
  /** Annotation on the connector; only rendered for two-node pipelines. */
  connectorLabel: string;
  /** True when there is no viable path (unsupported). */
  broken: boolean;
};

/** Data-flow topology of the selected route: source → (bridge?) → target. */
export function adapterRoutePipelineModel(input: {
  sourceTitle: string;
  sourceAgentId?: AgentId | null;
  credentialLabel: string;
  targetAgentId: AgentId;
  route: AdapterRoute;
  bridgeEndpoint?: string | null;
}): AdapterPipelineModel {
  const source: AdapterPipelineNode = {
    kind: 'source',
    title: input.sourceTitle,
    subtitle: input.credentialLabel,
    agentId: input.sourceAgentId ?? undefined,
  };
  const target: AdapterPipelineNode = {
    kind: 'target',
    title: agentDisplayName(input.targetAgentId),
    subtitle: '目标 Agent',
    agentId: input.targetAgentId,
  };
  if (input.route === 'local_bridge') {
    return {
      nodes: [
        source,
        {
          kind: 'bridge',
          title: '本地桥接',
          subtitle: input.bridgeEndpoint ?? '127.0.0.1 · 端口自动分配',
        },
        target,
      ],
      connectorLabel: '协议转换',
      broken: false,
    };
  }
  if (input.route === 'native_endpoint') {
    return { nodes: [source, target], connectorLabel: '直连 · 原生端点', broken: false };
  }
  if (input.route === 'config_sync') {
    return { nodes: [source, target], connectorLabel: '配置同步', broken: false };
  }
  return { nodes: [source, target], connectorLabel: '暂无可用路径', broken: true };
}

/** One-line path summary for the apply confirmation dialog. */
export function adapterApplySummaryLine(input: {
  sourceTitle: string;
  targetAgentId: AgentId;
  route: AdapterRoute;
}): string {
  const target = agentDisplayName(input.targetAgentId);
  return input.route === 'local_bridge'
    ? `${input.sourceTitle} → 本地桥接（127.0.0.1） → ${target}`
    : `${input.sourceTitle} → ${target}`;
}

// ---------- Managed profiles ----------

/** Fleet one-liner when there are at least two local bridges; `running` includes degraded. */
export function adapterBridgeFleetSummary(
  profiles: readonly Pick<AdapterProfile, 'id' | 'route'>[],
  bridgeStatuses: Record<string, { state: AdapterBridgeRuntimeState } | undefined>,
): { total: number; running: number; label: string } | null {
  const bridges = profiles.filter((profile) => profile.route === 'local_bridge');
  if (bridges.length < 2) return null;
  const running = bridges.filter((profile) => {
    const state = bridgeStatuses[profile.id]?.state;
    return state === 'running' || state === 'degraded';
  }).length;
  return {
    total: bridges.length,
    running,
    label: `${bridges.length} 个本机桥 · ${running} 个运行中 · 需保持托盘运行`,
  };
}

export type AdapterProfilePrimaryAction =
  | { kind: 'stop'; label: string }
  | { kind: 'start'; label: string };

/**
 * Row-level primary action. Direct routes have none. A degraded bridge still
 * owns its listener and must be stopped, not started again. A status-read
 * failure must not treat last-known `error` as a start failure.
 */
export function adapterProfilePrimaryAction(input: {
  route: AdapterProfile['route'];
  bridgeState?: AdapterBridgeRuntimeState;
  lastErrorCode?: string | null;
  statusUnavailable?: boolean;
}): AdapterProfilePrimaryAction | null {
  if (input.route !== 'local_bridge') return null;
  const ownsListener = input.bridgeState === 'running' || input.bridgeState === 'degraded';
  if (input.statusUnavailable) {
    return ownsListener
      ? { kind: 'stop', label: '停止' }
      : { kind: 'start', label: '启动' };
  }
  if (ownsListener) return { kind: 'stop', label: '停止' };
  const retry = input.bridgeState === 'error' || Boolean(input.lastErrorCode?.trim());
  return { kind: 'start', label: retry ? '重试启动' : '启动' };
}

/** Human-readable "source → target" one-liner for confirmations. */
export function adapterProfileFlowLabel(
  profile: Pick<AdapterProfile, 'sourceKind' | 'sourceId' | 'name' | 'targetAgentId'>,
  entries: readonly Pick<ConnectionEntry, 'source' | 'id' | 'title' | 'agentId'>[],
): string {
  const source = resolveAdapterProfileSource(profile, entries);
  return `${source.title} → ${agentDisplayName(profile.targetAgentId)}`;
}

/**
 * Recovery guidance for a needs_attention profile. Starting a bridge only
 * restores the runtime; it cannot repair inconsistent durable configuration,
 * so delete-and-recreate stays the explicit fallback.
 */
export function adapterProfileRecoveryGuide(profile: Pick<AdapterProfile, 'route' | 'status' | 'lastErrorCode'>): {
  summary: string;
  steps: string[];
} | null {
  if (profile.status !== 'needs_attention') return null;
  const code = profile.lastErrorCode?.trim();
  return {
    summary: code ? `上次未完成（${code}）。` : '上次可能未完成。',
    steps: [
      ...(profile.route === 'local_bridge'
        ? ['启动只恢复桥接运行时，不会修复配置不一致。']
        : []),
      '解除绑定后，到 Dashboard 重新连接。',
      '不会自动反复重试。',
    ],
  };
}
