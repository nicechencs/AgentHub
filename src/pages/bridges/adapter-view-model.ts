/**
 * Pure view-model helpers for the Bridges page: partition, single-layer
 * runtime status, source resolution, fleet, and recovery. No IO, no React.
 */
import type {
  AdapterBridgeRuntimeState,
  AdapterProfile,
} from '@/lib/backend/contracts/adapter';
import type { AgentId } from '@/lib/types';
import type { ConnectionEntry } from '@/lib/connection-entry';
import type { TranslateFn } from '@/lib/i18n';
import { formatRouteEndpointHttpUrl, routeEndpointPathForBinding } from '@/lib/route-endpoints';

export type AdapterStatusTone = 'success' | 'warning' | 'danger' | 'info' | 'muted';

/** `pulse` marks transient states (starting/stopping) for a breathing dot. */
export type AdapterStatusView = { label: string; tone: AdapterStatusTone; pulse?: boolean };

/**
 * Single-layer local-bridge runtime status. A failed status read is
 * "状态不可用" (the listener may still be fine) and must not be presented as
 * a start failure — even when last-known `bridgeState` is still `running`.
 */
export function bridgeRuntimeStatusView(input: {
  route: AdapterProfile['route'];
  bridgeState?: AdapterBridgeRuntimeState;
  statusUnavailable?: boolean;
}, t?: TranslateFn): AdapterStatusView | null {
  if (input.route !== 'local_bridge') return null;
  if (input.statusUnavailable) {
    return { label: t ? t('routes.runtime.unavailable') : '状态不可用', tone: 'muted' };
  }
  const state = input.bridgeState;
  if (state === 'running') return { label: t ? t('routes.runtime.running') : '运行中', tone: 'success' };
  if (state === 'starting') {
    return { label: t ? t('routes.runtime.starting') : '启动中', tone: 'info', pulse: true };
  }
  if (state === 'stopping') {
    return { label: t ? t('routes.runtime.stopping') : '停止中', tone: 'info', pulse: true };
  }
  if (state === 'degraded') return { label: t ? t('routes.runtime.degraded') : '已降级', tone: 'warning' };
  if (state === 'error') return { label: t ? t('routes.runtime.error') : '启动失败', tone: 'danger' };
  return { label: t ? t('routes.runtime.stopped') : '已停止', tone: 'muted' };
}

/** @deprecated Prefer {@link bridgeRuntimeStatusView}; same single-layer labels. */
export function adapterServiceStatusView(input: {
  route: AdapterProfile['route'];
  bridgeState?: AdapterBridgeRuntimeState;
  statusUnavailable?: boolean;
}, t?: TranslateFn): AdapterStatusView | null {
  return bridgeRuntimeStatusView(input, t);
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

// ---------- Managed profiles ----------

/** Fleet one-liner when there are at least two local bridges; `running` includes degraded. */
export function adapterBridgeFleetSummary(
  profiles: readonly Pick<AdapterProfile, 'id' | 'route'>[],
  bridgeStatuses: Record<string, { state: AdapterBridgeRuntimeState } | undefined>,
  t?: TranslateFn,
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
    label: t
      ? t('routes.fleetSummary', { total: bridges.length, running })
      : `${bridges.length} 个本机路由 · ${running} 个运行中 · 需保持托盘运行`,
  };
}

export type AdapterProfilePrimaryAction =
  | { kind: 'stop'; label: string }
  | { kind: 'start'; label: string };

/** A degraded bridge still owns its local listener and must be stopped, not started again. */
export function isBridgeStopCapable(
  state: AdapterBridgeRuntimeState | undefined,
): boolean {
  return state === 'running' || state === 'degraded';
}

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
}, t?: TranslateFn): AdapterProfilePrimaryAction | null {
  if (input.route !== 'local_bridge') return null;
  const ownsListener = isBridgeStopCapable(input.bridgeState);
  const stopLabel = t ? t('routes.action.stop') : '停止';
  const startLabel = t ? t('routes.action.start') : '启动';
  if (input.statusUnavailable) {
    return ownsListener
      ? { kind: 'stop', label: stopLabel }
      : { kind: 'start', label: startLabel };
  }
  if (ownsListener) return { kind: 'stop', label: stopLabel };
  const retry = input.bridgeState === 'error' || Boolean(input.lastErrorCode?.trim());
  return { kind: 'start', label: retry ? (t ? t('routes.action.retryStart') : '重试启动') : startLabel };
}

/** Human-readable "source → endpoint" one-liner for confirmations. */
export function adapterProfileFlowLabel(
  profile: Pick<AdapterProfile, 'sourceKind' | 'sourceId' | 'name' | 'targetAgentId' | 'ruleId' | 'localPort'>,
  entries: readonly Pick<ConnectionEntry, 'source' | 'id' | 'title' | 'agentId'>[],
): string {
  const source = resolveAdapterProfileSource(profile, entries);
  const path = routeEndpointPathForBinding({
    agentId: profile.targetAgentId,
    ruleId: profile.ruleId,
  });
  return `${source.title} → ${formatRouteEndpointHttpUrl({
    path,
    port: profile.localPort,
  })}`;
}

/**
 * Recovery guidance for a needs_attention profile. Starting a bridge only
 * restores the runtime; it cannot repair inconsistent durable configuration,
 * so delete-and-recreate stays the explicit fallback.
 */
export function adapterProfileRecoveryGuide(
  profile: Pick<AdapterProfile, 'route' | 'status' | 'lastErrorCode'>,
  t?: TranslateFn,
): {
  summary: string;
  steps: string[];
} | null {
  if (profile.status !== 'needs_attention') return null;
  const code = profile.lastErrorCode?.trim();
  return {
    summary: code
      ? (t ? t('routes.recovery.summaryWithCode', { code }) : `上次未完成（${code}）。`)
      : (t ? t('routes.recovery.summaryUnknown') : '上次可能未完成。'),
    steps: [
      ...(profile.route === 'local_bridge'
        ? [t ? t('routes.recovery.startOnlyRuntime') : '启动只会恢复转发服务，不会修好配置不一致。']
        : []),
      t ? t('routes.recovery.unbindReconnect') : '停止并还原后，到总览重新连接。',
      t ? t('routes.recovery.noAutoRetry') : '不会自动反复重试。',
    ],
  };
}
